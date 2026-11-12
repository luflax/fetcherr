use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::upstream::UpstreamError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0} is not configured yet, open Settings")]
    NotConfigured(&'static str),
    #[error("release expired, search again")]
    ReleaseExpired,
    #[error("{0} not found")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("missing or wrong API key")]
    Unauthorized,
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    TooManyRequests(String),
    #[error(transparent)]
    Upstream(#[from] UpstreamError),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotConfigured(_) => StatusCode::CONFLICT,
            Self::ReleaseExpired => StatusCode::GONE,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::Upstream(err) if err.not_found => StatusCode::NOT_FOUND,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured(_) => "not_configured",
            Self::ReleaseExpired => "release_expired",
            Self::NotFound(_) => "not_found",
            Self::Invalid(_) => "invalid",
            Self::Unauthorized => "unauthorized",
            Self::Conflict(_) => "conflict",
            Self::TooManyRequests(_) => "too_many_requests",
            Self::Upstream(err) if err.not_found => "not_found",
            Self::Upstream(_) => "upstream",
            Self::Internal(_) => "internal",
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        Self::Internal(format!("database: {err}"))
    }
}

/// Error payload returned by every API endpoint: `{"error": {"code", "message"}}`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if matches!(self, Self::Internal(_) | Self::Upstream(_)) {
            tracing::warn!("{self}");
        }
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code().to_string(),
                message: self.to_string(),
            },
        };
        (self.status(), Json(body)).into_response()
    }
}
