use axum::Json;
use axum::extract::{Path, State};

use crate::domain::MediaType;
use crate::error::{AppError, ErrorBody};
use crate::integrations;
use crate::settings::{
    DownloadClientSettingsInput, DownloadClientSettingsView, DownloadClientTestResult,
    IndexerSettingsInput, IndexerSettingsView, IndexerTestResult, MediaTypeSettingsInput,
    MediaTypeSettingsView,
};
use crate::state::AppState;

#[utoipa::path(
    get, path = "/api/settings/indexer", tag = "settings",
    responses((status = 200, body = IndexerSettingsView))
)]
pub async fn get_indexer(State(state): State<AppState>) -> Json<IndexerSettingsView> {
    Json((&state.settings.get().indexer).into())
}

#[utoipa::path(
    put, path = "/api/settings/indexer", tag = "settings",
    request_body = IndexerSettingsInput,
    responses(
        (status = 200, body = IndexerSettingsView),
        (status = 422, description = "Invalid settings", body = ErrorBody),
    )
)]
pub async fn put_indexer(
    State(state): State<AppState>,
    Json(input): Json<IndexerSettingsInput>,
) -> Result<Json<IndexerSettingsView>, AppError> {
    let settings = state
        .update_settings(|settings| {
            settings.indexer = input.merge_into(&settings.indexer)?;
            Ok(())
        })
        .await?;
    Ok(Json((&settings.indexer).into()))
}

/// Tests unsaved Prowlarr settings; a blank API key falls back to the stored one.
#[utoipa::path(
    post, path = "/api/settings/indexer/test", tag = "settings",
    request_body = IndexerSettingsInput,
    responses(
        (status = 200, description = "Connected", body = IndexerTestResult),
        (status = 502, description = "Prowlarr unreachable or rejected the key", body = ErrorBody),
    )
)]
pub async fn test_indexer(
    State(state): State<AppState>,
    Json(input): Json<IndexerSettingsInput>,
) -> Result<Json<IndexerTestResult>, AppError> {
    let candidate = input.merge_into(&state.settings.get().indexer)?;
    Ok(Json(
        integrations::test_prowlarr(&state.http, &candidate).await?,
    ))
}

#[utoipa::path(
    get, path = "/api/settings/download-client", tag = "settings",
    responses((status = 200, body = DownloadClientSettingsView))
)]
pub async fn get_download_client(
    State(state): State<AppState>,
) -> Json<DownloadClientSettingsView> {
    Json((&state.settings.get().download_client).into())
}

#[utoipa::path(
    put, path = "/api/settings/download-client", tag = "settings",
    request_body = DownloadClientSettingsInput,
    responses(
        (status = 200, body = DownloadClientSettingsView),
        (status = 422, description = "Invalid settings", body = ErrorBody),
    )
)]
pub async fn put_download_client(
    State(state): State<AppState>,
    Json(input): Json<DownloadClientSettingsInput>,
) -> Result<Json<DownloadClientSettingsView>, AppError> {
    let settings = state
        .update_settings(|settings| {
            settings.download_client = input.merge_into(&settings.download_client)?;
            Ok(())
        })
        .await?;
    Ok(Json((&settings.download_client).into()))
}

/// Tests unsaved qBittorrent settings; a blank password falls back to the stored one.
#[utoipa::path(
    post, path = "/api/settings/download-client/test", tag = "settings",
    request_body = DownloadClientSettingsInput,
    responses(
        (status = 200, description = "Connected", body = DownloadClientTestResult),
        (status = 502, description = "qBittorrent unreachable or login failed", body = ErrorBody),
    )
)]
pub async fn test_download_client(
    State(state): State<AppState>,
    Json(input): Json<DownloadClientSettingsInput>,
) -> Result<Json<DownloadClientTestResult>, AppError> {
    let candidate = input.merge_into(&state.settings.get().download_client)?;
    Ok(Json(
        integrations::test_qbittorrent(&state.http, &candidate).await?,
    ))
}

#[utoipa::path(
    get, path = "/api/settings/media-types", tag = "settings",
    responses((status = 200, body = Vec<MediaTypeSettingsView>))
)]
pub async fn get_media_types(State(state): State<AppState>) -> Json<Vec<MediaTypeSettingsView>> {
    Json(MediaTypeSettingsView::all(&state.settings.get().media))
}

/// Categories, download folder and qualities of one media type.
#[utoipa::path(
    put, path = "/api/settings/media-types/{media_type}", tag = "settings",
    params(("media_type" = MediaType, Path, description = "movies, series or software")),
    request_body = MediaTypeSettingsInput,
    responses(
        (status = 200, body = MediaTypeSettingsView),
        (status = 404, description = "Unknown media type", body = ErrorBody),
        (status = 422, description = "Invalid settings", body = ErrorBody),
    )
)]
pub async fn put_media_type(
    State(state): State<AppState>,
    Path(media_type): Path<String>,
    Json(input): Json<MediaTypeSettingsInput>,
) -> Result<Json<MediaTypeSettingsView>, AppError> {
    let media_type = MediaType::from_id(&media_type)
        .ok_or_else(|| AppError::NotFound(format!("media type '{media_type}'")))?;
    let validated = input.validate(media_type)?;
    let settings = state
        .update_settings(|settings| {
            *settings.media.get_mut(media_type) = validated;
            Ok(())
        })
        .await?;
    Ok(Json(MediaTypeSettingsView::new(
        media_type,
        settings.media.get(media_type),
    )))
}
