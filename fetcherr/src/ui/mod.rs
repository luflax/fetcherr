//! Server-rendered htmx UI. It never touches services: data and actions go through
//! [`ApiClient`] (the REST API, in-process) and, for live progress, the browser's WebSocket.

mod api_client;
mod assets;
mod connections;
mod downloads;
mod format;
mod layout_editor;
mod page;
mod search;
mod settings;

use axum::Router;
use axum::routing::{get, post};

pub use api_client::ApiClient;

pub fn router(api: ApiClient) -> Router {
    Router::new()
        .route("/", get(search::page))
        .route("/search/results", get(search::results))
        .route("/search/filters", get(search::filters))
        .route("/downloads", get(downloads::page).post(downloads::start))
        .route("/downloads/rows", get(downloads::rows))
        .route("/downloads/delete", post(downloads::delete))
        .route("/downloads/{id}/pause", post(downloads::pause))
        .route("/downloads/{id}/resume", post(downloads::resume))
        .route("/settings", get(settings::page))
        .route("/settings/indexer", post(settings::save_indexer))
        .route("/settings/indexer/test", post(settings::test_indexer))
        .route(
            "/settings/download-client",
            post(settings::save_download_client),
        )
        .route(
            "/settings/download-client/test",
            post(settings::test_download_client),
        )
        .route(
            "/settings/media/{media_type}",
            post(settings::save_media_type),
        )
        .route("/settings/tmdb", post(connections::save_tmdb))
        .route("/settings/tmdb/test", post(connections::test_tmdb))
        .route("/settings/omdb", post(connections::save_omdb))
        .route("/settings/jellyfin", post(connections::save_jellyfin))
        .route("/settings/jellyfin/test", post(connections::test_jellyfin))
        .route("/settings/library", post(connections::save_library))
        .route(
            "/settings/api-key/regenerate",
            post(connections::regenerate_api_key),
        )
        .route("/static/{*path}", get(assets::serve))
        .with_state(api)
}
