//! Prowlarr API v1 payloads. Source of truth: `docs/upstream/prowlarr-openapi.json`.

use serde::Deserialize;

/// `SystemResource` (only the fields Fetcherr reads).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemResource {
    pub version: Option<String>,
}

/// `IndexerCategory`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexerCategory {
    pub id: i32,
    pub name: Option<String>,
    #[serde(default)]
    pub sub_categories: Option<Vec<IndexerCategory>>,
}

/// `DownloadProtocol`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadProtocol {
    Unknown,
    Usenet,
    Torrent,
}

/// `ReleaseResource` (only the fields Fetcherr reads).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseResource {
    pub guid: Option<String>,
    #[serde(default)]
    pub age_hours: f64,
    #[serde(default)]
    pub size: i64,
    pub indexer_id: i32,
    pub indexer: Option<String>,
    pub title: Option<String>,
    pub publish_date: Option<String>,
    pub download_url: Option<String>,
    pub info_url: Option<String>,
    pub categories: Option<Vec<IndexerCategory>>,
    pub magnet_url: Option<String>,
    pub seeders: Option<i32>,
    pub leechers: Option<i32>,
    pub protocol: DownloadProtocol,
}
