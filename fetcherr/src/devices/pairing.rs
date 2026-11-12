//! Pairings in progress: short codes a device shows until someone approves them in Settings.
//! They live in memory only; a restart just means the device asks for a new code.

use std::sync::Mutex;
use std::time::Duration;

use subtle::ConstantTimeEq;

use super::DeviceId;
use crate::clock;

/// Long enough to find the Settings page and type the code; stale codes do not linger.
pub const PAIRING_TTL: Duration = Duration::from_secs(10 * 60);
/// Bounds memory, since starting a pairing needs no key.
const MAX_PENDING: usize = 32;
const CODE_LENGTH: usize = 6;
/// No `0`/`O` or `1`/`I`, which look alike on a TV screen.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Debug, Clone)]
pub struct Pairing {
    pub code: String,
    pub secret: String,
    pub name: Option<String>,
    pub expires_at: i64,
    pub approved: Option<Approval>,
}

#[derive(Debug, Clone)]
pub struct Approval {
    pub device_id: DeviceId,
    pub api_key: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PairingError {
    Full,
    Random(String),
}

#[derive(Default)]
pub struct Pairings {
    pending: Mutex<Vec<Pairing>>,
}

impl Pairings {
    pub fn start(&self, name: Option<String>) -> Result<Pairing, PairingError> {
        let mut pending = self.lock();
        let now = clock::now_unix();
        pending.retain(|pairing| pairing.expires_at > now);
        if pending.len() >= MAX_PENDING {
            return Err(PairingError::Full);
        }
        let code = loop {
            let code = random_code()?;
            if !pending.iter().any(|pairing| pairing.code == code) {
                break code;
            }
        };
        let pairing = Pairing {
            code,
            secret: random_hex(32)?,
            name,
            expires_at: now + PAIRING_TTL.as_secs() as i64,
            approved: None,
        };
        pending.push(pairing.clone());
        Ok(pairing)
    }

    /// The pairing for `secret`. Once approved it is handed out a single time and forgotten.
    pub fn poll(&self, secret: &str) -> Option<Pairing> {
        let mut pending = self.lock();
        let now = clock::now_unix();
        pending.retain(|pairing| pairing.expires_at > now);
        let index = pending
            .iter()
            .position(|pairing| bool::from(pairing.secret.as_bytes().ct_eq(secret.as_bytes())))?;
        if pending[index].approved.is_some() {
            Some(pending.swap_remove(index))
        } else {
            Some(pending[index].clone())
        }
    }

    /// The name the device asked for, if `code` is waiting for approval.
    pub fn find_waiting(&self, code: &str) -> Option<Option<String>> {
        let code = normalize_code(code);
        let now = clock::now_unix();
        self.lock()
            .iter()
            .find(|pairing| {
                pairing.code == code && pairing.approved.is_none() && pairing.expires_at > now
            })
            .map(|pairing| pairing.name.clone())
    }

    /// Marks `code` approved; false when it expired or was approved meanwhile.
    pub fn approve(&self, code: &str, approval: Approval) -> bool {
        let code = normalize_code(code);
        let now = clock::now_unix();
        let mut pending = self.lock();
        match pending.iter_mut().find(|pairing| {
            pairing.code == code && pairing.approved.is_none() && pairing.expires_at > now
        }) {
            Some(pairing) => {
                pairing.approved = Some(approval);
                true
            }
            None => false,
        }
    }

    /// Drops an approved pairing whose device was removed before it collected its key.
    pub fn forget_device(&self, id: DeviceId) {
        self.lock().retain(|pairing| {
            pairing
                .approved
                .as_ref()
                .is_none_or(|approval| approval.device_id != id)
        });
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Pairing>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Codes are typed by hand: ignore case, spaces and dashes.
pub fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn random_code() -> Result<String, PairingError> {
    let mut bytes = [0u8; CODE_LENGTH];
    getrandom::fill(&mut bytes).map_err(|err| PairingError::Random(err.to_string()))?;
    // 256 is a multiple of the alphabet's 32 letters, so every letter is equally likely.
    Ok(bytes
        .iter()
        .map(|byte| char::from(CODE_ALPHABET[usize::from(*byte) % CODE_ALPHABET.len()]))
        .collect())
}

fn random_hex(len: usize) -> Result<String, PairingError> {
    let mut bytes = vec![0u8; len];
    getrandom::fill(&mut bytes).map_err(|err| PairingError::Random(err.to_string()))?;
    Ok(hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approval() -> Approval {
        Approval {
            device_id: 7,
            api_key: "key".into(),
        }
    }

    #[test]
    fn codes_are_six_unambiguous_letters() {
        let code = random_code().unwrap();
        assert_eq!(code.len(), CODE_LENGTH);
        assert!(code.bytes().all(|c| CODE_ALPHABET.contains(&c)));
    }

    #[test]
    fn hands_out_the_key_once_after_approval() {
        let pairings = Pairings::default();
        let pairing = pairings.start(Some("TV".into())).unwrap();
        assert!(pairings.poll(&pairing.secret).unwrap().approved.is_none());
        assert!(pairings.poll("wrong").is_none());

        let typed = format!(
            " {}-{} ",
            &pairing.code[..3],
            pairing.code[3..].to_lowercase()
        );
        assert_eq!(pairings.find_waiting(&typed), Some(Some("TV".into())));
        assert!(pairings.approve(&typed, approval()));
        assert!(!pairings.approve(&typed, approval()), "approved twice");

        let polled = pairings.poll(&pairing.secret).unwrap();
        assert_eq!(polled.approved.unwrap().device_id, 7);
        assert!(
            pairings.poll(&pairing.secret).is_none(),
            "key handed out twice"
        );
    }

    #[test]
    fn limits_pending_pairings() {
        let pairings = Pairings::default();
        for _ in 0..MAX_PENDING {
            pairings.start(None).unwrap();
        }
        assert_eq!(pairings.start(None).unwrap_err(), PairingError::Full);
    }

    #[test]
    fn forgets_expired_pairings() {
        let pairings = Pairings::default();
        let pairing = pairings.start(None).unwrap();
        pairings.lock()[0].expires_at = clock::now_unix() - 1;
        assert!(pairings.poll(&pairing.secret).is_none());
        assert_eq!(pairings.find_waiting(&pairing.code), None);
    }
}
