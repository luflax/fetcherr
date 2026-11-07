//! User settings edited from the UI. Stored as JSON; secrets are write-only in the API.

mod store;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::Category;
use crate::error::AppError;

pub use store::SettingsStore;

pub const DEFAULT_POLL_INTERVAL_MS: u64 = 1000;
const POLL_INTERVAL_RANGE_MS: std::ops::RangeInclusive<u64> = 250..=60_000;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub indexer: IndexerSettings,
    pub download_client: DownloadClientSettings,
}

/// Prowlarr connection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexerSettings {
    pub url: String,
    pub api_key: String,
    pub categories: Vec<u32>,
}

/// qBittorrent connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadClientSettings {
    pub url: String,
    pub username: String,
    pub password: String,
    pub root_folder: String,
    pub poll_interval_ms: u64,
}

impl Default for DownloadClientSettings {
    fn default() -> Self {
        Self {
            url: String::new(),
            username: String::new(),
            password: String::new(),
            root_folder: String::new(),
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
        }
    }
}

impl IndexerSettings {
    pub fn is_configured(&self) -> bool {
        !self.url.trim().is_empty() && !self.api_key.is_empty()
    }
}

impl DownloadClientSettings {
    pub fn is_configured(&self) -> bool {
        !self.url.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IndexerSettingsView {
    pub url: String,
    pub has_api_key: bool,
    pub categories: Vec<u32>,
}

/// Omit `api_key` (or send it empty) to keep the stored one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IndexerSettingsInput {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default)]
    pub categories: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DownloadClientSettingsView {
    pub url: String,
    pub username: String,
    pub has_password: bool,
    pub root_folder: String,
    pub poll_interval_ms: u64,
}

/// Omit `password` (or send it empty) to keep the stored one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DownloadClientSettingsInput {
    pub url: String,
    #[serde(default)]
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default)]
    pub root_folder: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
}

fn default_poll_interval() -> u64 {
    DEFAULT_POLL_INTERVAL_MS
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IndexerTestResult {
    pub version: String,
    pub categories: Vec<Category>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DownloadClientTestResult {
    pub version: String,
}

impl From<&IndexerSettings> for IndexerSettingsView {
    fn from(settings: &IndexerSettings) -> Self {
        Self {
            url: settings.url.clone(),
            has_api_key: !settings.api_key.is_empty(),
            categories: settings.categories.clone(),
        }
    }
}

impl From<&DownloadClientSettings> for DownloadClientSettingsView {
    fn from(settings: &DownloadClientSettings) -> Self {
        Self {
            url: settings.url.clone(),
            username: settings.username.clone(),
            has_password: !settings.password.is_empty(),
            root_folder: settings.root_folder.clone(),
            poll_interval_ms: settings.poll_interval_ms,
        }
    }
}

impl IndexerSettingsInput {
    pub fn merge_into(self, current: &IndexerSettings) -> Result<IndexerSettings, AppError> {
        require_url("Prowlarr", &self.url)?;
        Ok(IndexerSettings {
            url: self.url.trim().to_string(),
            api_key: keep_secret_if_blank(self.api_key, &current.api_key),
            categories: self.categories,
        })
    }
}

impl DownloadClientSettingsInput {
    pub fn merge_into(
        self,
        current: &DownloadClientSettings,
    ) -> Result<DownloadClientSettings, AppError> {
        require_url("qBittorrent", &self.url)?;
        if !POLL_INTERVAL_RANGE_MS.contains(&self.poll_interval_ms) {
            return Err(AppError::Invalid(format!(
                "poll interval must be between {} and {} ms",
                POLL_INTERVAL_RANGE_MS.start(),
                POLL_INTERVAL_RANGE_MS.end()
            )));
        }
        Ok(DownloadClientSettings {
            url: self.url.trim().to_string(),
            username: self.username.trim().to_string(),
            password: keep_secret_if_blank(self.password, &current.password),
            root_folder: self.root_folder.trim().to_string(),
            poll_interval_ms: self.poll_interval_ms,
        })
    }
}

fn keep_secret_if_blank(new: Option<String>, current: &str) -> String {
    match new {
        Some(secret) if !secret.is_empty() => secret,
        _ => current.to_string(),
    }
}

fn require_url(service: &str, url: &str) -> Result<(), AppError> {
    let url = url.trim();
    let is_http = url.starts_with("http://") || url.starts_with("https://");
    if is_http {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "{service} URL must start with http:// or https://"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_secret_keeps_stored_one() {
        let current = IndexerSettings {
            api_key: "stored".into(),
            ..Default::default()
        };
        let input = IndexerSettingsInput {
            url: "http://prowlarr:9696".into(),
            api_key: Some(String::new()),
            categories: vec![2000],
        };
        let merged = input.merge_into(&current).unwrap();
        assert_eq!(merged.api_key, "stored");
        assert_eq!(merged.categories, vec![2000]);
    }

    #[test]
    fn new_secret_replaces_stored_one() {
        let current = DownloadClientSettings {
            password: "old".into(),
            ..Default::default()
        };
        let input = DownloadClientSettingsInput {
            url: "http://qbit:8080".into(),
            password: Some("new".into()),
            poll_interval_ms: 1000,
            ..Default::default()
        };
        assert_eq!(input.merge_into(&current).unwrap().password, "new");
    }

    #[test]
    fn view_hides_secrets() {
        let view = IndexerSettingsView::from(&IndexerSettings {
            api_key: "secret".into(),
            ..Default::default()
        });
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("secret"));
        assert!(view.has_api_key);
    }

    #[test]
    fn rejects_out_of_range_poll_interval() {
        let input = DownloadClientSettingsInput {
            url: "http://qbit:8080".into(),
            poll_interval_ms: 10,
            ..Default::default()
        };
        assert!(
            input
                .merge_into(&DownloadClientSettings::default())
                .is_err()
        );
    }

    #[test]
    fn rejects_url_without_scheme() {
        let input = IndexerSettingsInput {
            url: "prowlarr:9696".into(),
            ..Default::default()
        };
        assert!(input.merge_into(&IndexerSettings::default()).is_err());
    }
}
