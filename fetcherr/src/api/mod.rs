//! JSON REST API (+ OpenAPI/Swagger at `/docs`). The UI consumes this API too.
//! Every `/api` route needs the API key ([`auth`]); `/health` and `/docs` do not.

mod auth;
mod discover;
mod downloads;
mod events;
mod http_cache;
mod requests;
mod search;
mod settings;
mod system;
mod titles;
mod ws;

use axum::Router;
use axum::http::header;
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::get;
use serde::Serialize;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_swagger_ui::{Config, SwaggerUi};

use crate::state::AppState;

pub use auth::InternalCall;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Fetcherr API",
        description = "Browse TMDB, request movies and series, and follow them until they are in \
            Jellyfin. Every `/api` endpoint needs the API key (Settings → API key) in the \
            `X-Api-Key` header or the `api_key` query parameter.\n\n\
            Paginated endpoints answer `{page, total_pages, total_results, items}` with 20 items \
            per page.\n\n\
            Live events: WebSocket `GET /api/ws/events?requests=<id>,<id>&titles=<key>,<key>` \
            (see the `Event` schema). Download progress: `GET /api/ws/downloads?ids=<id>,<id>`."
    ),
    modifiers(&ApiKeySecurity),
    security(("api_key_header" = []), ("api_key_query" = [])),
    components(schemas(crate::events::Event)),
    tags(
        (name = "discover", description = "Configured pages and lists of titles"),
        (name = "titles", description = "Movies, series, people, search, ratings and releases"),
        (name = "requests", description = "Requests and their downloads"),
        (name = "events", description = "Live request progress and availability"),
        (name = "system", description = "Status and library sync"),
        (name = "settings", description = "Connections, library, layout and API key"),
        (name = "search", description = "Release search"),
        (name = "downloads", description = "Downloads managed by Fetcherr"),
        (name = "health", description = "Liveness"),
    )
)]
struct ApiDoc;

struct ApiKeySecurity;

impl Modify for ApiKeySecurity {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "api_key_header",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-Api-Key"))),
        );
        components.add_security_scheme(
            "api_key_query",
            SecurityScheme::ApiKey(ApiKey::Query(ApiKeyValue::new("api_key"))),
        );
    }
}

pub fn router(state: AppState) -> Router {
    let (router, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(health))
        .routes(routes!(system::status))
        .routes(routes!(system::sync_library))
        .routes(routes!(discover::pages))
        .routes(routes!(discover::page))
        .routes(routes!(discover::list))
        .routes(routes!(titles::movie))
        .routes(routes!(titles::movie_ratings))
        .routes(routes!(titles::movie_releases))
        .routes(routes!(titles::series))
        .routes(routes!(titles::season))
        .routes(routes!(titles::series_ratings))
        .routes(routes!(titles::series_releases))
        .routes(routes!(titles::person))
        .routes(routes!(titles::search))
        .routes(routes!(requests::create, requests::list))
        .routes(routes!(requests::get, requests::delete))
        .routes(routes!(requests::pause))
        .routes(routes!(requests::resume))
        .routes(routes!(requests::retry))
        .routes(routes!(events::events))
        .routes(routes!(settings::get_indexer, settings::put_indexer))
        .routes(routes!(settings::test_indexer))
        .routes(routes!(
            settings::get_download_client,
            settings::put_download_client
        ))
        .routes(routes!(settings::test_download_client))
        .routes(routes!(settings::get_media_types))
        .routes(routes!(settings::put_media_type))
        .routes(routes!(settings::get_tmdb, settings::put_tmdb))
        .routes(routes!(settings::test_tmdb))
        .routes(routes!(settings::get_omdb, settings::put_omdb))
        .routes(routes!(settings::get_jellyfin, settings::put_jellyfin))
        .routes(routes!(settings::test_jellyfin))
        .routes(routes!(settings::get_library, settings::put_library))
        .routes(routes!(settings::get_layout, settings::put_layout))
        .routes(routes!(settings::get_api_key))
        .routes(routes!(settings::regenerate_api_key))
        .routes(routes!(search::qualities))
        .routes(routes!(search::categories))
        .routes(routes!(search::search))
        .routes(routes!(downloads::start, downloads::list))
        .routes(routes!(downloads::get, downloads::delete))
        .routes(routes!(downloads::pause))
        .routes(routes!(downloads::resume))
        .split_for_parts();

    let spec = openapi.to_json().unwrap_or_else(|err| {
        tracing::error!("cannot serialize OpenAPI spec: {err}");
        "{}".to_string()
    });

    let settings = state.settings.clone();
    router
        .route("/api/ws/downloads", get(ws::downloads))
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            settings,
            auth::require_api_key,
        ))
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

#[utoipa::path(
    get, path = "/health", tag = "health",
    security(()),
    responses((status = 200, body = Health))
)]
async fn health() -> impl IntoResponse {
    axum::Json(Health { status: "ok" })
}
