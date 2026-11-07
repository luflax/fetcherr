use axum::Json;
use axum::extract::State;
use axum_extra::extract::Query;
use serde::Serialize;
use utoipa::ToSchema;

use crate::domain::{Category, Release, SearchQuery};
use crate::error::{AppError, ErrorBody};
use crate::quality::Quality;
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

/// Searches releases. Repeat `categories`, `qualities` or `indexer_ids` to pass several values.
/// Results only include allowed qualities, ranked in the media type's (or the requested)
/// quality order first, then by `sort`.
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
    let settings = state.settings.get();
    let releases = crate::search::search(
        &state.integrations(),
        &state.releases,
        &settings.media,
        &query,
    )
    .await?;
    Ok(Json(releases))
}

#[derive(Serialize, ToSchema)]
pub struct QualityOption {
    pub id: Quality,
    pub label: &'static str,
}

/// Every quality Fetcherr detects, best first.
#[utoipa::path(
    get, path = "/api/qualities", tag = "search",
    responses((status = 200, body = Vec<QualityOption>))
)]
pub async fn qualities() -> Json<Vec<QualityOption>> {
    let options = Quality::ALL
        .into_iter()
        .map(|quality| QualityOption {
            id: quality,
            label: quality.label(),
        })
        .collect();
    Json(options)
}
