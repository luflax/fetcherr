use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;

use super::http_cache::cached_json;
use crate::catalog::{DiscoverPageView, PageTab};
use crate::domain::{Page, PageQuery};
use crate::error::{AppError, ErrorBody};
use crate::state::AppState;
use crate::titles::Card;

/// The discover pages (tabs), in their configured order.
#[utoipa::path(
    get, path = "/api/discover/pages", tag = "discover",
    responses(
        (status = 200, body = Vec<PageTab>, headers(("ETag" = String), ("Cache-Control" = String))),
        (status = 304, description = "Unchanged since the `If-None-Match` ETag"),
    )
)]
pub async fn pages(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    cached_json(&headers, &state.catalog.pages())
}

/// A page's lists, in order, each with its first page of cards.
#[utoipa::path(
    get, path = "/api/discover/pages/{id}", tag = "discover",
    params(("id" = String, Path, description = "Page id")),
    responses(
        (status = 200, body = DiscoverPageView, headers(("ETag" = String), ("Cache-Control" = String))),
        (status = 304, description = "Unchanged since the `If-None-Match` ETag"),
        (status = 404, description = "Unknown page", body = ErrorBody),
        (status = 409, description = "TMDB not configured", body = ErrorBody),
    )
)]
pub async fn page(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    cached_json(&headers, &state.catalog.page(&id).await?)
}

/// One list, page by page (20 cards per page).
#[utoipa::path(
    get, path = "/api/discover/lists/{id}", tag = "discover",
    params(("id" = String, Path, description = "List id"), PageQuery),
    responses(
        (status = 200, body = Page<Card>, headers(("ETag" = String), ("Cache-Control" = String))),
        (status = 304, description = "Unchanged since the `If-None-Match` ETag"),
        (status = 404, description = "Unknown list", body = ErrorBody),
        (status = 409, description = "TMDB not configured", body = ErrorBody),
        (status = 502, description = "TMDB failed", body = ErrorBody),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(page): Query<PageQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    cached_json(&headers, &state.catalog.list(&id, page.page()).await?)
}
