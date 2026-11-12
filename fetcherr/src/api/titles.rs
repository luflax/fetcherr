use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::catalog::{MovieView, PersonView, RatingsView, SearchResult, SeasonView, SeriesView};
use crate::domain::Page;
use crate::error::{AppError, ErrorBody};
use crate::search::TitleRelease;
use crate::state::AppState;
use crate::titles::{TitleKey, TitleKind};

#[utoipa::path(
    get, path = "/api/movies/{tmdb_id}", tag = "titles",
    params(("tmdb_id" = u32, Path)),
    responses(
        (status = 200, description = "Details, availability, requests and related lists", body = MovieView),
        (status = 404, description = "Unknown on TMDB", body = ErrorBody),
        (status = 409, description = "TMDB not configured", body = ErrorBody),
    )
)]
pub async fn movie(
    State(state): State<AppState>,
    Path(tmdb_id): Path<u32>,
) -> Result<Json<MovieView>, AppError> {
    Ok(Json(state.catalog.movie(tmdb_id).await?))
}

#[utoipa::path(
    get, path = "/api/series/{tmdb_id}", tag = "titles",
    params(("tmdb_id" = u32, Path)),
    responses(
        (status = 200, description = "Details and seasons with availability", body = SeriesView),
        (status = 404, description = "Unknown on TMDB", body = ErrorBody),
        (status = 409, description = "TMDB not configured", body = ErrorBody),
    )
)]
pub async fn series(
    State(state): State<AppState>,
    Path(tmdb_id): Path<u32>,
) -> Result<Json<SeriesView>, AppError> {
    Ok(Json(state.catalog.series(tmdb_id).await?))
}

#[utoipa::path(
    get, path = "/api/series/{tmdb_id}/seasons/{season}", tag = "titles",
    params(("tmdb_id" = u32, Path), ("season" = u32, Path)),
    responses(
        (status = 200, description = "Episodes with availability and request state", body = SeasonView),
        (status = 404, description = "Unknown on TMDB", body = ErrorBody),
    )
)]
pub async fn season(
    State(state): State<AppState>,
    Path((tmdb_id, season)): Path<(u32, u32)>,
) -> Result<Json<SeasonView>, AppError> {
    Ok(Json(state.catalog.season(tmdb_id, season).await?))
}

#[utoipa::path(
    get, path = "/api/movies/{tmdb_id}/ratings", tag = "titles",
    params(("tmdb_id" = u32, Path)),
    responses((status = 200, description = "TMDB, IMDb and Rotten Tomatoes scores", body = RatingsView))
)]
pub async fn movie_ratings(
    State(state): State<AppState>,
    Path(tmdb_id): Path<u32>,
) -> Result<Json<RatingsView>, AppError> {
    Ok(Json(
        state
            .catalog
            .ratings(TitleKey::new(TitleKind::Movie, tmdb_id))
            .await?,
    ))
}

#[utoipa::path(
    get, path = "/api/series/{tmdb_id}/ratings", tag = "titles",
    params(("tmdb_id" = u32, Path)),
    responses((status = 200, description = "TMDB, IMDb and Rotten Tomatoes scores", body = RatingsView))
)]
pub async fn series_ratings(
    State(state): State<AppState>,
    Path(tmdb_id): Path<u32>,
) -> Result<Json<RatingsView>, AppError> {
    Ok(Json(
        state
            .catalog
            .ratings(TitleKey::new(TitleKind::Series, tmdb_id))
            .await?,
    ))
}

#[utoipa::path(
    get, path = "/api/people/{tmdb_id}", tag = "titles",
    params(("tmdb_id" = u32, Path)),
    responses(
        (status = 200, description = "Biography, photo and credits", body = PersonView),
        (status = 404, description = "Unknown on TMDB", body = ErrorBody),
    )
)]
pub async fn person(
    State(state): State<AppState>,
    Path(tmdb_id): Path<u32>,
) -> Result<Json<PersonView>, AppError> {
    Ok(Json(state.catalog.person(tmdb_id).await?))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TitleSearchQuery {
    query: String,
    #[serde(default = "first_page")]
    #[param(minimum = 1, default = 1)]
    page: u32,
}

fn first_page() -> u32 {
    1
}

/// Movies, series and people matching a text.
#[utoipa::path(
    get, path = "/api/titles/search", tag = "titles",
    params(TitleSearchQuery),
    responses(
        (status = 200, body = Page<SearchResult>),
        (status = 422, description = "Empty query", body = ErrorBody),
    )
)]
pub async fn search(
    State(state): State<AppState>,
    Query(query): Query<TitleSearchQuery>,
) -> Result<Json<Page<SearchResult>>, AppError> {
    Ok(Json(
        state
            .catalog
            .search(&query.query, query.page.max(1))
            .await?,
    ))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReleaseScope {
    season: Option<u32>,
    /// Needs `season`.
    episode: Option<u32>,
}

/// Releases of a movie for the manual picker. Post a `release_id` with `POST /api/requests`.
#[utoipa::path(
    get, path = "/api/movies/{tmdb_id}/releases", tag = "titles",
    params(("tmdb_id" = u32, Path)),
    responses(
        (status = 200, body = Vec<TitleRelease>),
        (status = 409, description = "TMDB or Prowlarr not configured", body = ErrorBody),
    )
)]
pub async fn movie_releases(
    State(state): State<AppState>,
    Path(tmdb_id): Path<u32>,
) -> Result<Json<Vec<TitleRelease>>, AppError> {
    let key = TitleKey::new(TitleKind::Movie, tmdb_id);
    Ok(Json(state.requests.releases(key, None, None).await?))
}

/// Releases of a series, a season or an episode for the manual picker.
#[utoipa::path(
    get, path = "/api/series/{tmdb_id}/releases", tag = "titles",
    params(("tmdb_id" = u32, Path), ReleaseScope),
    responses(
        (status = 200, body = Vec<TitleRelease>),
        (status = 409, description = "TMDB or Prowlarr not configured", body = ErrorBody),
        (status = 422, description = "Episode without season", body = ErrorBody),
    )
)]
pub async fn series_releases(
    State(state): State<AppState>,
    Path(tmdb_id): Path<u32>,
    Query(scope): Query<ReleaseScope>,
) -> Result<Json<Vec<TitleRelease>>, AppError> {
    let key = TitleKey::new(TitleKind::Series, tmdb_id);
    Ok(Json(
        state
            .requests
            .releases(key, scope.season, scope.episode)
            .await?,
    ))
}
