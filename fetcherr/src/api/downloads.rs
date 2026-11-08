use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;

use serde::Deserialize;
use utoipa::IntoParams;

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
    let settings = state.settings.get();
    let id = downloads::start(&state.integrations(), &state.releases, &settings, request).await?;
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

#[utoipa::path(
    post, path = "/api/downloads/{id}/pause", tag = "downloads",
    params(("id" = String, Path, description = "Torrent info-hash")),
    responses(
        (status = 204, description = "Paused"),
        (status = 404, description = "Unknown download", body = ErrorBody),
        (status = 502, description = "qBittorrent failed", body = ErrorBody),
    )
)]
pub async fn pause(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id = DownloadId::try_from(id)?;
    downloads::pause(&state.integrations(), &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/api/downloads/{id}/resume", tag = "downloads",
    params(("id" = String, Path, description = "Torrent info-hash")),
    responses(
        (status = 204, description = "Resumed"),
        (status = 404, description = "Unknown download", body = ErrorBody),
        (status = 502, description = "qBittorrent failed", body = ErrorBody),
    )
)]
pub async fn resume(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id = DownloadId::try_from(id)?;
    downloads::resume(&state.integrations(), &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DeleteOptions {
    /// Also delete the downloaded files from disk.
    #[serde(default)]
    delete_files: bool,
}

/// Removes the download from qBittorrent. Files are kept unless `delete_files=true`.
#[utoipa::path(
    delete, path = "/api/downloads/{id}", tag = "downloads",
    params(("id" = String, Path, description = "Torrent info-hash"), DeleteOptions),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Unknown download", body = ErrorBody),
        (status = 502, description = "qBittorrent failed", body = ErrorBody),
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(options): Query<DeleteOptions>,
) -> Result<StatusCode, AppError> {
    let id = DownloadId::try_from(id)?;
    downloads::delete(&state.integrations(), &id, options.delete_files).await?;
    Ok(StatusCode::NO_CONTENT)
}
