//! JSON REST API (+ OpenAPI/Swagger at `/docs`). The UI consumes this API too.

mod downloads;
mod search;
mod settings;
mod ws;

use axum::Router;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_swagger_ui::{Config, SwaggerUi};

use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Fetcherr API",
        description = "Search releases through Prowlarr and download them with qBittorrent.\n\n\
            Live progress: WebSocket `GET /api/ws/downloads?ids=<id>,<id>` (see add-on docs)."
    ),
    tags(
        (name = "settings", description = "Indexer and download client connections"),
        (name = "search", description = "Release search"),
        (name = "downloads", description = "Downloads managed by Fetcherr"),
        (name = "health", description = "Liveness"),
    )
)]
struct ApiDoc;

pub fn router(state: AppState) -> Router {
    let (router, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(health))
        .routes(routes!(settings::get_indexer, settings::put_indexer))
        .routes(routes!(settings::test_indexer))
        .routes(routes!(
            settings::get_download_client,
            settings::put_download_client
        ))
        .routes(routes!(settings::test_download_client))
        .routes(routes!(search::categories))
        .routes(routes!(search::search))
        .routes(routes!(downloads::start, downloads::list))
        .routes(routes!(downloads::get))
        .split_for_parts();

    let spec = openapi.to_json().unwrap_or_else(|err| {
        tracing::error!("cannot serialize OpenAPI spec: {err}");
        "{}".to_string()
    });

    router
        .route("/api/ws/downloads", get(ws::downloads))
        .with_state(state)
        .route(
            "/docs/openapi.json",
            get(move || async move { ([(header::CONTENT_TYPE, "application/json")], spec) }),
        )
        // Relative spec URL so Swagger also works behind the Home Assistant Ingress prefix.
        .merge(SwaggerUi::new("/docs").config(Config::new(["openapi.json"])))
}

#[derive(Serialize, ToSchema)]
struct Health {
    status: &'static str,
}

#[utoipa::path(get, path = "/health", tag = "health", responses((status = 200, body = Health)))]
async fn health() -> impl IntoResponse {
    axum::Json(Health { status: "ok" })
}
