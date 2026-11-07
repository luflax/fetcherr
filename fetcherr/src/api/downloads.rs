use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::domain::{Download, DownloadId, DownloadStarted, StartDownload};
use crate::downloads;
use crate::error::{AppError, ErrorBody};
use crate::state::AppState;

/// Sends a search result to the download client. The returned id is the torrent info-hash.
#[utoipa::path(
    post, path = "/api/downloads", tag = "downloads",
    request_body = StartDownload,
    responses(
        (status = 202, description = "Download accepted", body = DownloadStarted),
        (status = 409, description = "qBittorrent or Prowlarr not configured", body = ErrorBody),
        (status = 410, description = "Release expired, search again", body = ErrorBody),
        (status = 502, description = "Upstream failed", body = ErrorBody),
    )
)]
pub async fn start(
    State(state): State<AppState>,
    Json(request): Json<StartDownload>,
) -> Result<(StatusCode, Json<DownloadStarted>), AppError> {
    let root_folder = state.settings.get().download_client.root_folder.clone();
    let id = downloads::start(
        &state.integrations(),
        &state.releases,
        &root_folder,
        request,
    )
    .await?;
    Ok((StatusCode::ACCEPTED, Json(DownloadStarted { id })))
}

#[utoipa::path(
    get, path = "/api/downloads", tag = "downloads",
    responses(
        (status = 200, body = Vec<Download>),
        (status = 409, description = "qBittorrent not configured", body = ErrorBody),
        (status = 502, description = "qBittorrent failed", body = ErrorBody),
    )
)]
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Download>>, AppError> {
    Ok(Json(downloads::list(&state.integrations()).await?))
}

#[utoipa::path(
    get, path = "/api/downloads/{id}", tag = "downloads",
    params(("id" = String, Path, description = "Torrent info-hash")),
    responses(
        (status = 200, body = Download),
        (status = 404, description = "Unknown download", body = ErrorBody),
        (status = 502, description = "qBittorrent failed", body = ErrorBody),
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Download>, AppError> {
    let id = DownloadId::try_from(id)?;
    Ok(Json(downloads::get(&state.integrations(), &id).await?))
}
