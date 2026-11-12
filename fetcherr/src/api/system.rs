use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;
use utoipa::ToSchema;

use crate::integrations::ConfiguredServices;
use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct SystemStatus {
    pub version: &'static str,
    /// Which services have usable settings.
    pub services: ConfiguredServices,
}

#[utoipa::path(
    get, path = "/api/system/status", tag = "system",
    responses((status = 200, body = SystemStatus))
)]
pub async fn status(State(state): State<AppState>) -> Json<SystemStatus> {
    Json(SystemStatus {
        version: env!("CARGO_PKG_VERSION"),
        services: state.integrations().configured(),
    })
}

/// Reads the Jellyfin library again now instead of at the next interval.
#[utoipa::path(
    post, path = "/api/library/sync", tag = "system",
    responses((status = 202, description = "Sync started"))
)]
pub async fn sync_library(State(state): State<AppState>) -> StatusCode {
    state.library.sync_now();
    StatusCode::ACCEPTED
}
