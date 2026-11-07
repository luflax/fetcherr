mod hello;

use axum::Router;
use axum::routing::get;

use crate::AppState;

/// Assembles the HTTP API. Add one module per upstream service and nest it here.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(hello::health))
        .route("/api/hello", get(hello::hello))
        .with_state(state)
}
