use axum::Json;
use axum::extract::State;
use axum_extra::extract::Query;

use crate::domain::{Category, Release, SearchQuery};
use crate::error::{AppError, ErrorBody};
use crate::state::AppState;

#[utoipa::path(
    get, path = "/api/indexer/categories", tag = "search",
    responses(
        (status = 200, body = Vec<Category>),
        (status = 409, description = "Prowlarr not configured", body = ErrorBody),
        (status = 502, description = "Prowlarr failed", body = ErrorBody),
    )
)]
pub async fn categories(State(state): State<AppState>) -> Result<Json<Vec<Category>>, AppError> {
    Ok(Json(state.integrations().prowlarr()?.categories().await?))
}

/// Searches releases. Repeat `categories` / `indexer_ids` to pass several values.
#[utoipa::path(
    get, path = "/api/search", tag = "search",
    params(SearchQuery),
    responses(
        (status = 200, body = Vec<Release>),
        (status = 409, description = "Prowlarr not configured", body = ErrorBody),
        (status = 502, description = "Prowlarr failed", body = ErrorBody),
    )
)]
pub async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<Release>>, AppError> {
    let default_categories = state.settings.get().indexer.categories.clone();
    let releases = crate::search::search(
        &state.integrations(),
        &state.releases,
        &default_categories,
        &query,
    )
    .await?;
    Ok(Json(releases))
}
