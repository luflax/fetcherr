//! Pairing devices with a code, and managing the paired ones.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::devices::{
    Device, DeviceId, PairedDevice, PairingApproval, PairingPending, PairingStart, PairingStatus,
    PairingTicket,
};
use crate::error::{AppError, ErrorBody};
use crate::state::AppState;

/// Starts pairing a device that has no key yet (no key needed). Show `code` on screen and poll
/// `GET /api/auth/pair/{secret}` every `poll_interval_secs` until it answers `200`.
#[utoipa::path(
    post, path = "/api/auth/pair", tag = "devices",
    security(()),
    request_body(content = Option<PairingStart>, description = "Optional suggested name"),
    responses(
        (status = 201, body = PairingTicket),
        (status = 429, description = "Too many pairings in progress", body = ErrorBody),
    )
)]
pub async fn start_pairing(
    State(state): State<AppState>,
    input: Option<Json<PairingStart>>,
) -> Result<(StatusCode, Json<PairingTicket>), AppError> {
    let input = input.map(|Json(input)| input).unwrap_or_default();
    let ticket = state.devices.start_pairing(input)?;
    Ok((StatusCode::CREATED, Json(ticket)))
}

/// Polls a pairing (no key needed): `202` while the code waits for approval, then `200` with
/// the device's API key, exactly once.
#[utoipa::path(
    get, path = "/api/auth/pair/{secret}", tag = "devices",
    security(()),
    params(("secret" = String, Path, description = "The `secret` from `POST /api/auth/pair`")),
    responses(
        (status = 200, description = "Approved; keep the key", body = PairedDevice),
        (status = 202, description = "Waiting for the code to be entered", body = PairingPending),
        (status = 404, description = "Unknown, expired or already collected", body = ErrorBody),
    )
)]
pub async fn poll_pairing(
    State(state): State<AppState>,
    Path(secret): Path<String>,
) -> Result<Response, AppError> {
    Ok(match state.devices.poll(&secret)? {
        PairingStatus::Pending(pending) => (StatusCode::ACCEPTED, Json(pending)).into_response(),
        PairingStatus::Paired(paired) => Json(paired).into_response(),
    })
}

#[utoipa::path(
    get, path = "/api/devices", tag = "devices",
    responses((status = 200, body = Vec<Device>))
)]
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<Device>>, AppError> {
    Ok(Json(state.devices.list().await?))
}

/// Approves the code a device shows; the device then collects its own API key.
#[utoipa::path(
    post, path = "/api/devices/pair", tag = "devices",
    request_body = PairingApproval,
    responses(
        (status = 201, body = Device),
        (status = 404, description = "Unknown or expired code", body = ErrorBody),
    )
)]
pub async fn approve(
    State(state): State<AppState>,
    Json(input): Json<PairingApproval>,
) -> Result<(StatusCode, Json<Device>), AppError> {
    let device = state.devices.approve(input).await?;
    Ok((StatusCode::CREATED, Json(device)))
}

/// Removes a device; its key stops working at once.
#[utoipa::path(
    delete, path = "/api/devices/{id}", tag = "devices",
    params(("id" = i64, Path)),
    responses(
        (status = 204, description = "Removed"),
        (status = 404, body = ErrorBody),
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<DeviceId>,
) -> Result<StatusCode, AppError> {
    state.devices.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
