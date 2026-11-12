use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::domain::{Page, PageQuery};
use crate::error::{AppError, ErrorBody};
use crate::requests::{NewRequest, RequestDetail, RequestId, RequestState, RequestSummary};
use crate::state::AppState;

/// Requests a movie, a season or episodes. The release search runs in the background; follow
/// it with `GET /api/requests/{id}` or the `/api/ws/events` WebSocket.
#[utoipa::path(
    post, path = "/api/requests", tag = "requests",
    request_body = NewRequest,
    responses(
        (status = 202, description = "Request created", body = RequestDetail),
        (status = 200, description = "Already requested: the existing request", body = RequestDetail),
        (status = 409, description = "TMDB not configured", body = ErrorBody),
        (status = 410, description = "The chosen release expired, search again", body = ErrorBody),
        (status = 422, description = "Invalid scope or release", body = ErrorBody),
    )
)]
pub async fn create(
    State(state): State<AppState>,
    Json(input): Json<NewRequest>,
) -> Result<(StatusCode, Json<RequestDetail>), AppError> {
    let (request, created) = state.requests.create(input).await?;
    let status = if created {
        StatusCode::ACCEPTED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(request)))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RequestFilter {
    #[param(inline)]
    state: Option<RequestState>,
}

/// Requests, newest first, 20 per page.
#[utoipa::path(
    get, path = "/api/requests", tag = "requests",
    params(RequestFilter, PageQuery),
    responses((status = 200, body = Page<RequestSummary>))
)]
pub async fn list(
    State(state): State<AppState>,
    Query(filter): Query<RequestFilter>,
    Query(page): Query<PageQuery>,
) -> Result<Json<Page<RequestSummary>>, AppError> {
    Ok(Json(state.requests.list(filter.state, page).await?))
}

#[utoipa::path(
    get, path = "/api/requests/{id}", tag = "requests",
    params(("id" = i64, Path)),
    responses(
        (status = 200, description = "The request with its torrents and imported files", body = RequestDetail),
        (status = 404, description = "Unknown request", body = ErrorBody),
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<RequestId>,
) -> Result<Json<RequestDetail>, AppError> {
    Ok(Json(state.requests.get(id).await?))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DeleteOptions {
    /// Also delete the downloaded data and the files imported into the library.
    #[serde(default)]
    delete_files: bool,
}

/// Removes a request and its torrents from qBittorrent.
#[utoipa::path(
    delete, path = "/api/requests/{id}", tag = "requests",
    params(("id" = i64, Path), DeleteOptions),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Unknown request", body = ErrorBody),
        (status = 502, description = "qBittorrent failed", body = ErrorBody),
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<RequestId>,
    Query(options): Query<DeleteOptions>,
) -> Result<StatusCode, AppError> {
    state.requests.delete(id, options.delete_files).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/api/requests/{id}/pause", tag = "requests",
    params(("id" = i64, Path)),
    responses(
        (status = 200, body = RequestDetail),
        (status = 409, description = "Not downloading", body = ErrorBody),
    )
)]
pub async fn pause(
    State(state): State<AppState>,
    Path(id): Path<RequestId>,
) -> Result<Json<RequestDetail>, AppError> {
    Ok(Json(state.requests.pause(id).await?))
}

#[utoipa::path(
    post, path = "/api/requests/{id}/resume", tag = "requests",
    params(("id" = i64, Path)),
    responses(
        (status = 200, body = RequestDetail),
        (status = 409, description = "Not paused", body = ErrorBody),
    )
)]
pub async fn resume(
    State(state): State<AppState>,
    Path(id): Path<RequestId>,
) -> Result<Json<RequestDetail>, AppError> {
    Ok(Json(state.requests.resume(id).await?))
}

/// Searches again for a failed or not-found request.
#[utoipa::path(
    post, path = "/api/requests/{id}/retry", tag = "requests",
    params(("id" = i64, Path)),
    responses(
        (status = 200, body = RequestDetail),
        (status = 409, description = "Neither failed nor not found", body = ErrorBody),
    )
)]
pub async fn retry(
    State(state): State<AppState>,
    Path(id): Path<RequestId>,
) -> Result<Json<RequestDetail>, AppError> {
    Ok(Json(state.requests.retry(id).await?))
}
