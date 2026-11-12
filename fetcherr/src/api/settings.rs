use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::domain::MediaType;
use crate::error::{AppError, ErrorBody};
use crate::integrations;
use crate::settings::{
    ApiKeyView, ApiSettings, DownloadClientSettingsInput, DownloadClientSettingsView,
    DownloadClientTestResult, IndexerSettingsInput, IndexerSettingsView, IndexerTestResult,
    JellyfinSettingsInput, JellyfinSettingsView, Layout, LayoutInput, LibrarySettings,
    MediaTypeSettingsInput, MediaTypeSettingsView, OmdbSettingsInput, OmdbSettingsView,
    TmdbSettingsInput, TmdbSettingsView, VersionTestResult,
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

#[utoipa::path(
    get, path = "/api/settings/tmdb", tag = "settings",
    responses((status = 200, body = TmdbSettingsView))
)]
pub async fn get_tmdb(State(state): State<AppState>) -> Json<TmdbSettingsView> {
    Json((&state.settings.get().tmdb).into())
}

/// TMDB key (v3 API key or v4 read access token), language and region.
#[utoipa::path(
    put, path = "/api/settings/tmdb", tag = "settings",
    request_body = TmdbSettingsInput,
    responses(
        (status = 200, body = TmdbSettingsView),
        (status = 422, description = "Invalid settings", body = ErrorBody),
    )
)]
pub async fn put_tmdb(
    State(state): State<AppState>,
    Json(input): Json<TmdbSettingsInput>,
) -> Result<Json<TmdbSettingsView>, AppError> {
    let settings = state
        .update_settings(|settings| {
            settings.tmdb = input.merge_into(&settings.tmdb)?;
            Ok(())
        })
        .await?;
    Ok(Json((&settings.tmdb).into()))
}

/// Tests unsaved TMDB settings; a blank key falls back to the stored one.
#[utoipa::path(
    post, path = "/api/settings/tmdb/test", tag = "settings",
    request_body = TmdbSettingsInput,
    responses(
        (status = 204, description = "Connected"),
        (status = 502, description = "TMDB unreachable or rejected the key", body = ErrorBody),
    )
)]
pub async fn test_tmdb(
    State(state): State<AppState>,
    Json(input): Json<TmdbSettingsInput>,
) -> Result<StatusCode, AppError> {
    let candidate = input.merge_into(&state.settings.get().tmdb)?;
    integrations::test_tmdb(&state.http, &candidate).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get, path = "/api/settings/omdb", tag = "settings",
    responses((status = 200, body = OmdbSettingsView))
)]
pub async fn get_omdb(State(state): State<AppState>) -> Json<OmdbSettingsView> {
    Json((&state.settings.get().omdb).into())
}

/// OMDb key, for IMDb and Rotten Tomatoes ratings.
#[utoipa::path(
    put, path = "/api/settings/omdb", tag = "settings",
    request_body = OmdbSettingsInput,
    responses(
        (status = 200, body = OmdbSettingsView),
        (status = 422, description = "Invalid settings", body = ErrorBody),
    )
)]
pub async fn put_omdb(
    State(state): State<AppState>,
    Json(input): Json<OmdbSettingsInput>,
) -> Result<Json<OmdbSettingsView>, AppError> {
    let settings = state
        .update_settings(|settings| {
            settings.omdb = input.merge_into(&settings.omdb)?;
            Ok(())
        })
        .await?;
    Ok(Json((&settings.omdb).into()))
}

#[utoipa::path(
    get, path = "/api/settings/jellyfin", tag = "settings",
    responses((status = 200, body = JellyfinSettingsView))
)]
pub async fn get_jellyfin(State(state): State<AppState>) -> Json<JellyfinSettingsView> {
    Json((&state.settings.get().jellyfin).into())
}

/// Jellyfin connection. Saving it syncs the library right away.
#[utoipa::path(
    put, path = "/api/settings/jellyfin", tag = "settings",
    request_body = JellyfinSettingsInput,
    responses(
        (status = 200, body = JellyfinSettingsView),
        (status = 422, description = "Invalid settings", body = ErrorBody),
    )
)]
pub async fn put_jellyfin(
    State(state): State<AppState>,
    Json(input): Json<JellyfinSettingsInput>,
) -> Result<Json<JellyfinSettingsView>, AppError> {
    let settings = state
        .update_settings(|settings| {
            settings.jellyfin = input.merge_into(&settings.jellyfin)?;
            Ok(())
        })
        .await?;
    Ok(Json((&settings.jellyfin).into()))
}

/// Tests unsaved Jellyfin settings; a blank key falls back to the stored one.
#[utoipa::path(
    post, path = "/api/settings/jellyfin/test", tag = "settings",
    request_body = JellyfinSettingsInput,
    responses(
        (status = 200, description = "Connected", body = VersionTestResult),
        (status = 502, description = "Jellyfin unreachable or rejected the key", body = ErrorBody),
    )
)]
pub async fn test_jellyfin(
    State(state): State<AppState>,
    Json(input): Json<JellyfinSettingsInput>,
) -> Result<Json<VersionTestResult>, AppError> {
    let candidate = input.merge_into(&state.settings.get().jellyfin)?;
    Ok(Json(
        integrations::test_jellyfin(&state.http, &candidate).await?,
    ))
}

#[utoipa::path(
    get, path = "/api/settings/library", tag = "settings",
    responses((status = 200, body = LibrarySettings))
)]
pub async fn get_library(State(state): State<AppState>) -> Json<LibrarySettings> {
    Json(state.settings.get().library.clone())
}

/// Library folders, import mode (hardlink or move), path mappings and naming templates.
#[utoipa::path(
    put, path = "/api/settings/library", tag = "settings",
    request_body = LibrarySettings,
    responses(
        (status = 200, body = LibrarySettings),
        (status = 422, description = "Invalid settings", body = ErrorBody),
    )
)]
pub async fn put_library(
    State(state): State<AppState>,
    Json(input): Json<LibrarySettings>,
) -> Result<Json<LibrarySettings>, AppError> {
    let validated = input.validated()?;
    let settings = state
        .update_settings(|settings| {
            settings.library = validated;
            Ok(())
        })
        .await?;
    Ok(Json(settings.library.clone()))
}

#[utoipa::path(
    get, path = "/api/settings/layout", tag = "settings",
    responses((status = 200, body = Layout))
)]
pub async fn get_layout(State(state): State<AppState>) -> Json<Layout> {
    Json(state.settings.get().layout.clone())
}

/// Replaces the discover layout. Pages and lists keep the order given; missing ids are made
/// from the names.
#[utoipa::path(
    put, path = "/api/settings/layout", tag = "settings",
    request_body = LayoutInput,
    responses(
        (status = 200, body = Layout),
        (status = 422, description = "Invalid layout", body = ErrorBody),
    )
)]
pub async fn put_layout(
    State(state): State<AppState>,
    Json(input): Json<LayoutInput>,
) -> Result<Json<Layout>, AppError> {
    let layout = input.validated()?;
    let settings = state
        .update_settings(|settings| {
            settings.layout = layout;
            Ok(())
        })
        .await?;
    Ok(Json(settings.layout.clone()))
}

/// The API key (this endpoint itself needs it, or the built-in UI).
#[utoipa::path(
    get, path = "/api/settings/api-key", tag = "settings",
    responses((status = 200, body = ApiKeyView))
)]
pub async fn get_api_key(State(state): State<AppState>) -> Json<ApiKeyView> {
    Json(ApiKeyView {
        key: state.settings.get().api.key.clone(),
    })
}

/// Replaces the API key; the old one stops working at once.
#[utoipa::path(
    post, path = "/api/settings/api-key/regenerate", tag = "settings",
    responses((status = 200, description = "The new key", body = ApiKeyView))
)]
pub async fn regenerate_api_key(
    State(state): State<AppState>,
) -> Result<Json<ApiKeyView>, AppError> {
    let key = ApiSettings::generate_key()?;
    let settings = state
        .update_settings(|settings| {
            settings.api.key = key;
            Ok(())
        })
        .await?;
    Ok(Json(ApiKeyView {
        key: settings.api.key.clone(),
    }))
}
