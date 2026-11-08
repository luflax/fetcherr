//! qBittorrent WebUI API v2 payloads. Source of truth: `docs/upstream/qbittorrent-webui-api-5.0.md`.

use serde::Deserialize;

/// Element of the `/api/v2/torrents/info` array (only the fields Fetcherr reads).
#[derive(Debug, Deserialize)]
pub struct TorrentInfo {
    pub hash: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub completed: i64,
    #[serde(default)]
    pub dlspeed: i64,
    #[serde(default)]
    pub upspeed: i64,
    #[serde(default)]
    pub eta: i64,
    #[serde(default)]
    pub num_seeds: i64,
    #[serde(default)]
    pub num_leechs: i64,
    #[serde(default)]
    pub save_path: String,
    #[serde(default)]
    pub category: String,
}
