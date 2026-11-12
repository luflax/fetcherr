//! Devices paired with a code (TVs, phones), each with its own API key so it can be removed
//! on its own.
//!
//! The device starts a pairing and shows the code, the user enters the code in Settings, and
//! the device, polling with its secret, then collects its key once.

mod pairing;
mod store;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::clock;
use crate::error::AppError;
use crate::settings::ApiSettings;
use pairing::{Approval, PairingError, Pairings};

pub type DeviceId = i64;

/// How often a device should poll its pairing.
const POLL_INTERVAL_SECS: u64 = 2;
/// `last_seen_at` is written at most this often per device.
const LAST_SEEN_RESOLUTION_SECS: i64 = 60;
const MAX_NAME_CHARS: usize = 64;
const DEFAULT_NAME: &str = "Device";

/// A paired device. Its key is only ever shown to the device itself, when pairing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Device {
    pub id: DeviceId,
    pub name: String,
    /// RFC 3339.
    pub created_at: String,
    /// RFC 3339; absent until the device first uses its key.
    pub last_seen_at: Option<String>,
}

/// Optional body of `POST /api/auth/pair`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(default)]
pub struct PairingStart {
    /// Suggested name, e.g. "Living room TV"; the user can change it when approving.
    pub name: Option<String>,
}

/// What the device shows and keeps while waiting.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PairingTicket {
    /// Six characters to show on screen, e.g. `K7Q4M2`.
    #[schema(example = "K7Q4M2")]
    pub code: String,
    /// Keep it private: poll `GET /api/auth/pair/{secret}` with it.
    pub secret: String,
    /// RFC 3339; after it, start again.
    pub expires: String,
    pub expires_in_secs: u64,
    pub poll_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PairingPending {
    #[schema(example = "pending")]
    pub status: String,
    /// RFC 3339.
    pub expires: String,
}

/// The device's own API key, handed out once.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PairedDevice {
    pub api_key: String,
    pub device_id: DeviceId,
}

pub enum PairingStatus {
    Pending(PairingPending),
    Paired(PairedDevice),
}

/// Body of `POST /api/devices/pair`.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
pub struct PairingApproval {
    /// The code the device shows; case, spaces and dashes are ignored.
    pub code: String,
    /// Overrides the name the device suggested.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone)]
pub struct Devices {
    inner: Arc<Inner>,
}

struct Inner {
    pool: SqlitePool,
    /// SHA-256 of each device key → device.
    keys: RwLock<HashMap<String, DeviceId>>,
    last_seen: Mutex<HashMap<DeviceId, i64>>,
    pairings: Pairings,
}

impl Devices {
    pub async fn open(pool: SqlitePool) -> Result<Self, AppError> {
        let keys = store::key_hashes(&pool).await?.into_iter().collect();
        Ok(Self {
            inner: Arc::new(Inner {
                pool,
                keys: RwLock::new(keys),
                last_seen: Mutex::default(),
                pairings: Pairings::default(),
            }),
        })
    }

    /// The device `key` belongs to, noting that it was seen.
    pub fn authenticate(&self, key: &str) -> Option<DeviceId> {
        let id = *self.read_keys().get(&hash_key(key))?;
        self.touch(id);
        Some(id)
    }

    pub fn start_pairing(&self, input: PairingStart) -> Result<PairingTicket, AppError> {
        let pairing = self
            .inner
            .pairings
            .start(input.name.as_deref().and_then(clean_name))
            .map_err(pairing_error)?;
        Ok(PairingTicket {
            code: pairing.code,
            secret: pairing.secret,
            expires: clock::rfc3339(pairing.expires_at),
            expires_in_secs: pairing::PAIRING_TTL.as_secs(),
            poll_interval_secs: POLL_INTERVAL_SECS,
        })
    }

    pub fn poll(&self, secret: &str) -> Result<PairingStatus, AppError> {
        let pairing = self
            .inner
            .pairings
            .poll(secret)
            .ok_or_else(|| AppError::NotFound("pairing (expired or already used)".into()))?;
        Ok(match pairing.approved {
            Some(approval) => PairingStatus::Paired(PairedDevice {
                api_key: approval.api_key,
                device_id: approval.device_id,
            }),
            None => PairingStatus::Pending(PairingPending {
                status: "pending".into(),
                expires: clock::rfc3339(pairing.expires_at),
            }),
        })
    }

    /// Creates the device for the code a device is showing.
    pub async fn approve(&self, input: PairingApproval) -> Result<Device, AppError> {
        let not_found = || AppError::NotFound("pairing code (expired or mistyped)".into());
        let suggested = self
            .inner
            .pairings
            .find_waiting(&input.code)
            .ok_or_else(not_found)?;
        let name = input
            .name
            .as_deref()
            .and_then(clean_name)
            .or(suggested)
            .unwrap_or_else(|| DEFAULT_NAME.into());
        let api_key = ApiSettings::generate_key()?;
        let key_hash = hash_key(&api_key);
        let now = clock::now_unix();
        let id = store::insert(&self.inner.pool, &name, &key_hash, now).await?;
        if !self.inner.pairings.approve(
            &input.code,
            Approval {
                device_id: id,
                api_key,
            },
        ) {
            store::delete(&self.inner.pool, id).await?;
            return Err(not_found());
        }
        self.write_keys().insert(key_hash, id);
        Ok(Device {
            id,
            name,
            created_at: clock::rfc3339(now),
            last_seen_at: None,
        })
    }

    pub async fn list(&self) -> Result<Vec<Device>, AppError> {
        let rows = store::list(&self.inner.pool).await?;
        let seen = self.lock_last_seen();
        Ok(rows
            .into_iter()
            .map(|row| {
                let last_seen = seen.get(&row.id).copied().or(row.last_seen_at);
                device(row, last_seen)
            })
            .collect())
    }

    /// Removes the device; its key stops working at once.
    pub async fn delete(&self, id: DeviceId) -> Result<(), AppError> {
        self.write_keys().retain(|_, device| *device != id);
        self.inner.pairings.forget_device(id);
        self.lock_last_seen().remove(&id);
        if store::delete(&self.inner.pool, id).await? {
            Ok(())
        } else {
            Err(AppError::NotFound(format!("device {id}")))
        }
    }

    fn touch(&self, id: DeviceId) {
        let now = clock::now_unix();
        {
            let mut seen = self.lock_last_seen();
            let last = seen.entry(id).or_insert(0);
            if now - *last < LAST_SEEN_RESOLUTION_SECS {
                return;
            }
            *last = now;
        }
        let pool = self.inner.pool.clone();
        tokio::spawn(async move {
            if let Err(err) = store::set_last_seen(&pool, id, now).await {
                tracing::warn!("cannot record when device {id} was seen: {err}");
            }
        });
    }

    fn read_keys(&self) -> std::sync::RwLockReadGuard<'_, HashMap<String, DeviceId>> {
        self.inner
            .keys
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_keys(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<String, DeviceId>> {
        self.inner
            .keys
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_last_seen(&self) -> std::sync::MutexGuard<'_, HashMap<DeviceId, i64>> {
        self.inner
            .last_seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn device(row: store::DeviceRow, last_seen: Option<i64>) -> Device {
    Device {
        id: row.id,
        name: row.name,
        created_at: clock::rfc3339(row.created_at),
        last_seen_at: last_seen.map(clock::rfc3339),
    }
}

fn hash_key(key: &str) -> String {
    hex::encode(Sha256::digest(key.as_bytes()))
}

/// Trimmed, at most [`MAX_NAME_CHARS`], or `None` when blank.
fn clean_name(name: &str) -> Option<String> {
    let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
    let name = name.trim_end().to_string();
    (!name.is_empty()).then_some(name)
}

fn pairing_error(err: PairingError) -> AppError {
    match err {
        PairingError::Full => {
            AppError::TooManyRequests("too many pairings in progress, try again later".into())
        }
        PairingError::Random(err) => {
            AppError::Internal(format!("cannot generate a pairing code: {err}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_names() {
        assert_eq!(clean_name("  TV  "), Some("TV".into()));
        assert_eq!(clean_name("   "), None);
        assert_eq!(clean_name(&"x".repeat(100)).unwrap().len(), MAX_NAME_CHARS);
    }
}
