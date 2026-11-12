//! Metadata and media-server connections: TMDB, OMDb and Jellyfin.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{keep_secret_if_blank, optional_url, require_url};
use crate::error::AppError;

const DEFAULT_LANGUAGE: &str = "en-US";
pub const DEFAULT_SYNC_MINUTES: u32 = 15;
const SYNC_MINUTES_RANGE: std::ops::RangeInclusive<u32> = 1..=1440;

/// TMDB connection. `url` is empty for the official API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TmdbSettings {
    pub url: String,
    /// v3 API key or v4 read access token.
    pub api_key: String,
    pub language: String,
    /// ISO 3166-1 country for release dates and "now playing"; empty for worldwide.
    pub region: String,
}

impl Default for TmdbSettings {
    fn default() -> Self {
        Self {
            url: String::new(),
            api_key: String::new(),
            language: DEFAULT_LANGUAGE.into(),
            region: String::new(),
        }
    }
}

impl TmdbSettings {
    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TmdbSettingsView {
    /// Empty uses `https://api.themoviedb.org`.
    pub url: String,
    pub has_api_key: bool,
    pub language: String,
    pub region: String,
}

/// Omit `api_key` (or send it empty) to keep the stored one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct TmdbSettingsInput {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub language: String,
    pub region: String,
}

impl From<&TmdbSettings> for TmdbSettingsView {
    fn from(settings: &TmdbSettings) -> Self {
        Self {
            url: settings.url.clone(),
            has_api_key: !settings.api_key.is_empty(),
            language: settings.language.clone(),
            region: settings.region.clone(),
        }
    }
}

impl TmdbSettingsInput {
    pub fn merge_into(self, current: &TmdbSettings) -> Result<TmdbSettings, AppError> {
        let language = self.language.trim();
        let region = self.region.trim().to_ascii_uppercase();
        if !region.is_empty()
            && !(region.len() == 2 && region.bytes().all(|b| b.is_ascii_alphabetic()))
        {
            return Err(AppError::Invalid(
                "region must be a two-letter country code".into(),
            ));
        }
        Ok(TmdbSettings {
            url: optional_url("TMDB", &self.url)?,
            api_key: keep_secret_if_blank(self.api_key, &current.api_key),
            language: if language.is_empty() {
                DEFAULT_LANGUAGE.into()
            } else {
                language.into()
            },
            region,
        })
    }
}

/// OMDb connection, used for IMDb and Rotten Tomatoes ratings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OmdbSettings {
    pub url: String,
    pub api_key: String,
}

impl OmdbSettings {
    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct OmdbSettingsView {
    /// Empty uses `https://www.omdbapi.com`.
    pub url: String,
    pub has_api_key: bool,
}

/// Omit `api_key` (or send it empty) to keep the stored one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct OmdbSettingsInput {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

impl From<&OmdbSettings> for OmdbSettingsView {
    fn from(settings: &OmdbSettings) -> Self {
        Self {
            url: settings.url.clone(),
            has_api_key: !settings.api_key.is_empty(),
        }
    }
}

impl OmdbSettingsInput {
    pub fn merge_into(self, current: &OmdbSettings) -> Result<OmdbSettings, AppError> {
        Ok(OmdbSettings {
            url: optional_url("OMDb", &self.url)?,
            api_key: keep_secret_if_blank(self.api_key, &current.api_key),
        })
    }
}

/// Jellyfin connection: availability comes from its library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct JellyfinSettings {
    pub url: String,
    pub api_key: String,
    pub sync_interval_minutes: u32,
}

impl Default for JellyfinSettings {
    fn default() -> Self {
        Self {
            url: String::new(),
            api_key: String::new(),
            sync_interval_minutes: DEFAULT_SYNC_MINUTES,
        }
    }
}

impl JellyfinSettings {
    pub fn is_configured(&self) -> bool {
        !self.url.trim().is_empty() && !self.api_key.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct JellyfinSettingsView {
    pub url: String,
    pub has_api_key: bool,
    pub sync_interval_minutes: u32,
}

/// Omit `api_key` (or send it empty) to keep the stored one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct JellyfinSettingsInput {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default = "default_sync_minutes")]
    pub sync_interval_minutes: u32,
}

fn default_sync_minutes() -> u32 {
    DEFAULT_SYNC_MINUTES
}

impl From<&JellyfinSettings> for JellyfinSettingsView {
    fn from(settings: &JellyfinSettings) -> Self {
        Self {
            url: settings.url.clone(),
            has_api_key: !settings.api_key.is_empty(),
            sync_interval_minutes: settings.sync_interval_minutes,
        }
    }
}

impl JellyfinSettingsInput {
    pub fn merge_into(self, current: &JellyfinSettings) -> Result<JellyfinSettings, AppError> {
        require_url("Jellyfin", &self.url)?;
        if !SYNC_MINUTES_RANGE.contains(&self.sync_interval_minutes) {
            return Err(AppError::Invalid(format!(
                "sync interval must be between {} and {} minutes",
                SYNC_MINUTES_RANGE.start(),
                SYNC_MINUTES_RANGE.end()
            )));
        }
        Ok(JellyfinSettings {
            url: self.url.trim().to_string(),
            api_key: keep_secret_if_blank(self.api_key, &current.api_key),
            sync_interval_minutes: self.sync_interval_minutes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct VersionTestResult {
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmdb_defaults_language_and_validates_region() {
        let input = TmdbSettingsInput {
            api_key: Some("key".into()),
            region: "br".into(),
            ..Default::default()
        };
        let saved = input.merge_into(&TmdbSettings::default()).unwrap();
        assert_eq!(saved.language, "en-US");
        assert_eq!(saved.region, "BR");

        let bad = TmdbSettingsInput {
            region: "Brazil".into(),
            ..Default::default()
        };
        assert!(bad.merge_into(&TmdbSettings::default()).is_err());
    }

    #[test]
    fn jellyfin_keeps_key_and_checks_interval() {
        let current = JellyfinSettings {
            api_key: "stored".into(),
            ..Default::default()
        };
        let input = JellyfinSettingsInput {
            url: "http://jellyfin:8096".into(),
            api_key: None,
            sync_interval_minutes: 15,
        };
        assert_eq!(
            input.clone().merge_into(&current).unwrap().api_key,
            "stored"
        );
        let too_often = JellyfinSettingsInput {
            sync_interval_minutes: 0,
            ..input
        };
        assert!(too_often.merge_into(&current).is_err());
    }
}
