//! Provider-agnostic types shared by services, the REST API contract and the UI.

use std::fmt;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use utoipa::ToSchema;

use crate::error::AppError;

/// Opaque handle to a search result, valid while the release cache remembers it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct ReleaseId(String);

impl ReleaseId {
    pub fn from_guid(indexer_id: i32, guid: &str) -> Self {
        let digest = Sha1::digest(format!("{indexer_id}:{guid}").as_bytes());
        Self(hex::encode(&digest[..10]))
    }
}

impl fmt::Display for ReleaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Release {
    pub release_id: ReleaseId,
    pub title: String,
    pub indexer: String,
    pub indexer_id: i32,
    /// Size in bytes.
    pub size: u64,
    pub seeders: u32,
    pub leechers: u32,
    /// RFC 3339 publish date as reported by the indexer.
    pub published_at: String,
    pub age_hours: f64,
    pub categories: Vec<u32>,
    pub info_url: Option<String>,
}

/// Where the torrent of a release can be fetched from. Never leaves the server:
/// download URLs embed the indexer API key.
#[derive(Debug, Clone, PartialEq)]
pub enum ReleaseSource {
    Magnet(String),
    TorrentUrl(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FoundRelease {
    pub release: Release,
    pub source: ReleaseSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Category {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    #[schema(no_recursion)]
    pub sub_categories: Vec<Category>,
}

/// Torrent v1 info-hash in lowercase hex; doubles as the download identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "String", into = "String")]
pub struct DownloadId(String);

impl DownloadId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for DownloadId {
    type Error = AppError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let value = value.to_ascii_lowercase();
        let is_sha1_hex = value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit());
        if is_sha1_hex {
            Ok(Self(value))
        } else {
            Err(AppError::Invalid(format!(
                "'{value}' is not a torrent info-hash"
            )))
        }
    }
}

impl From<DownloadId> for String {
    fn from(id: DownloadId) -> Self {
        id.0
    }
}

impl fmt::Display for DownloadId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    Queued,
    FetchingMetadata,
    Downloading,
    Stalled,
    Paused,
    Checking,
    Moving,
    Seeding,
    Completed,
    Error,
    Unknown,
}

impl DownloadState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::FetchingMetadata => "Metadata",
            Self::Downloading => "Downloading",
            Self::Stalled => "Stalled",
            Self::Paused => "Paused",
            Self::Checking => "Checking",
            Self::Moving => "Moving",
            Self::Seeding => "Seeding",
            Self::Completed => "Completed",
            Self::Error => "Error",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Download {
    pub id: DownloadId,
    pub name: String,
    pub state: DownloadState,
    /// Fraction from 0.0 to 1.0.
    pub progress: f64,
    pub size: u64,
    pub downloaded: u64,
    /// Bytes per second.
    pub dl_speed: u64,
    /// Bytes per second.
    pub ul_speed: u64,
    /// Seconds left, absent when unknown.
    pub eta_secs: Option<u64>,
    pub seeds: u32,
    pub peers: u32,
    pub save_path: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    #[default]
    Seeders,
    Size,
    Age,
}

/// Search filters. `query`, `categories`, `indexer_ids` and `limit` are applied by the indexer;
/// the rest by Fetcherr. Empty `categories` falls back to the configured defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchQuery {
    #[serde(default)]
    pub query: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub indexer_ids: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_seeders: Option<u32>,
    /// Bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_size: Option<u64>,
    /// Bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,
    #[serde(default)]
    #[param(inline)]
    pub sort: SortBy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct StartDownload {
    pub release_id: ReleaseId,
    /// Overrides the configured root folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DownloadStarted {
    pub id: DownloadId,
}
