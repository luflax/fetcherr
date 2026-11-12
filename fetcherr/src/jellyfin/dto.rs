//! Jellyfin API payloads. Source of truth: `docs/upstream/jellyfin-openapi-stable.json`.

use std::collections::HashMap;

use serde::Deserialize;

/// `BaseItemDtoQueryResult` (only the fields Fetcherr reads).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemsResult {
    #[serde(default)]
    pub items: Vec<BaseItem>,
    #[serde(default)]
    pub total_record_count: u32,
}

/// `BaseItemDto` (only the fields Fetcherr reads).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItem {
    pub id: String,
    #[serde(rename = "Type")]
    pub kind: Option<String>,
    pub provider_ids: Option<HashMap<String, Option<String>>>,
    pub series_id: Option<String>,
    pub parent_index_number: Option<i32>,
    pub index_number: Option<i32>,
    pub index_number_end: Option<i32>,
}

/// `SystemInfo` (only the fields Fetcherr reads).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SystemInfo {
    pub server_name: Option<String>,
    pub version: Option<String>,
}
