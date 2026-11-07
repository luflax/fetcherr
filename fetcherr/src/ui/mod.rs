//! Server-rendered htmx UI. It never touches services: data and actions go through
//! [`ApiClient`] (the REST API, in-process) and, for live progress, the browser's WebSocket.

mod api_client;
mod assets;
mod downloads;
mod format;
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
        .route("/downloads", get(downloads::page).post(downloads::start))
        .route("/downloads/rows", get(downloads::rows))
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
        .route("/static/{*path}", get(assets::serve))
        .with_state(api)
}
