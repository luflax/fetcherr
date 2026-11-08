//! User settings edited from the UI. Stored as JSON; secrets are write-only in the API.

mod store;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::{Category, MediaType};
use crate::error::AppError;
use crate::quality::Quality;

pub use store::SettingsStore;

pub const DEFAULT_POLL_INTERVAL_MS: u64 = 1000;
const POLL_INTERVAL_RANGE_MS: std::ops::RangeInclusive<u64> = 250..=60_000;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub indexer: IndexerSettings,
    pub download_client: DownloadClientSettings,
    pub media: MediaSettings,
}

/// Prowlarr connection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexerSettings {
    pub url: String,
    pub api_key: String,
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

/// Per media type search and download preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaSettings {
    pub movies: MediaTypeSettings,
    pub series: MediaTypeSettings,
    pub software: MediaTypeSettings,
}

impl Default for MediaSettings {
    fn default() -> Self {
        Self {
            movies: MediaTypeSettings::with_categories(vec![2000]),
            series: MediaTypeSettings::with_categories(vec![5000]),
            software: MediaTypeSettings::with_categories(vec![4000]),
        }
    }
}

impl MediaSettings {
    pub fn get(&self, media_type: MediaType) -> &MediaTypeSettings {
        match media_type {
            MediaType::Movies => &self.movies,
            MediaType::Series => &self.series,
            MediaType::Software => &self.software,
        }
    }

    pub fn get_mut(&mut self, media_type: MediaType) -> &mut MediaTypeSettings {
        match media_type {
            MediaType::Movies => &mut self.movies,
            MediaType::Series => &mut self.series,
            MediaType::Software => &mut self.software,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaTypeSettings {
    /// Indexer categories searched for this media type.
    pub categories: Vec<u32>,
    /// Empty falls back to the download client's root folder.
    pub download_folder: String,
    /// Allowed qualities, most preferred first. Empty allows every quality, unranked.
    pub allowed_qualities: Vec<Quality>,
}

impl MediaTypeSettings {
    fn with_categories(categories: Vec<u32>) -> Self {
        Self {
            categories,
            ..Default::default()
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
}

/// Omit `api_key` (or send it empty) to keep the stored one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct IndexerSettingsInput {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
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
pub struct MediaTypeSettingsView {
    pub media_type: MediaType,
    pub label: String,
    /// Software has no qualities.
    pub has_qualities: bool,
    pub categories: Vec<u32>,
    /// Empty uses the download client's root folder.
    pub download_folder: String,
    /// Most preferred first; search results are ranked in this order. Empty allows every quality.
    pub allowed_qualities: Vec<Quality>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MediaTypeSettingsInput {
    #[serde(default)]
    pub categories: Vec<u32>,
    #[serde(default)]
    pub download_folder: String,
    /// Most preferred first; search results are ranked in this order. Empty allows every quality.
    #[serde(default)]
    pub allowed_qualities: Vec<Quality>,
}

impl MediaTypeSettingsView {
    pub fn new(media_type: MediaType, settings: &MediaTypeSettings) -> Self {
        Self {
            media_type,
            label: media_type.label().to_string(),
            has_qualities: media_type.has_qualities(),
            categories: settings.categories.clone(),
            download_folder: settings.download_folder.clone(),
            allowed_qualities: settings.allowed_qualities.clone(),
        }
    }

    pub fn all(media: &MediaSettings) -> Vec<Self> {
        MediaType::ALL
            .into_iter()
            .map(|media_type| Self::new(media_type, media.get(media_type)))
            .collect()
    }
}

impl MediaTypeSettingsInput {
    pub fn validate(self, media_type: MediaType) -> Result<MediaTypeSettings, AppError> {
        if !self.allowed_qualities.is_empty() && !media_type.has_qualities() {
            return Err(AppError::Invalid(format!(
                "{} has no qualities",
                media_type.label()
            )));
        }
        Ok(MediaTypeSettings {
            categories: self.categories,
            download_folder: self.download_folder.trim().to_string(),
            allowed_qualities: without_duplicates(self.allowed_qualities),
        })
    }
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

/// Keeps the first occurrence of each quality, so the preference order survives.
fn without_duplicates(qualities: Vec<Quality>) -> Vec<Quality> {
    let mut unique = Vec::with_capacity(qualities.len());
    for quality in qualities {
        if !unique.contains(&quality) {
            unique.push(quality);
        }
    }
    unique
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
        };
        let merged = input.merge_into(&current).unwrap();
        assert_eq!(merged.api_key, "stored");
    }

    #[test]
    fn software_has_no_qualities() {
        let input = MediaTypeSettingsInput {
            allowed_qualities: vec![Quality::Web1080p],
            ..Default::default()
        };
        assert!(input.clone().validate(MediaType::Software).is_err());
        assert!(input.validate(MediaType::Movies).is_ok());
    }

    #[test]
    fn keeps_quality_preference_order_without_duplicates() {
        let input = MediaTypeSettingsInput {
            allowed_qualities: vec![Quality::Web2160p, Quality::Bluray1080p, Quality::Web2160p],
            download_folder: " /media/movies ".into(),
            ..Default::default()
        };
        let saved = input.validate(MediaType::Movies).unwrap();
        assert_eq!(
            saved.allowed_qualities,
            vec![Quality::Web2160p, Quality::Bluray1080p]
        );
        assert_eq!(saved.download_folder, "/media/movies");
    }

    #[test]
    fn old_settings_files_still_load() {
        let json = r#"{"indexer": {"url": "http://p", "api_key": "k", "categories": [2000]}}"#;
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.indexer.api_key, "k");
        assert_eq!(settings.media.movies.categories, vec![2000]);
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
