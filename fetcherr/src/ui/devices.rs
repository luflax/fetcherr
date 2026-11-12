//! Settings → Devices: enter the code a TV shows, and remove paired devices.

use askama::Template;
use axum::extract::{Path, State};
use axum::http::Method;
use axum::response::Response;
use axum_extra::extract::Form;
use serde::Deserialize;

use super::api_client::{ApiClient, ApiError};
use super::page::{Toast, render, render_all};
use super::settings::keep_form;
use crate::devices::{Device, DeviceId, PairingApproval};

#[derive(Template)]
#[template(path = "partials/devices_card.html")]
pub struct DevicesCard {
    devices: Vec<DeviceRow>,
}

struct DeviceRow {
    id: DeviceId,
    name: String,
    paired: String,
    last_seen: String,
}

impl DevicesCard {
    pub fn new(devices: Vec<Device>) -> Self {
        Self {
            devices: devices
                .into_iter()
                .map(|device| DeviceRow {
                    id: device.id,
                    paired: readable_time(&device.created_at),
                    last_seen: device
                        .last_seen_at
                        .as_deref()
                        .map_or_else(|| "never".into(), readable_time),
                    name: device.name,
                })
                .collect(),
        }
    }
}

pub async fn fetch(api: &ApiClient) -> Result<Vec<Device>, ApiError> {
    api.get::<Vec<Device>>("/api/devices").await
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct PairForm {
    code: String,
    name: String,
}

pub async fn pair(State(api): State<ApiClient>, Form(form): Form<PairForm>) -> Response {
    let input = PairingApproval {
        code: form.code,
        name: Some(form.name),
    };
    match api.post::<_, Device>("/api/devices/pair", &input).await {
        Ok(device) => {
            let toast = Toast::success(format!(
                "{} is paired; it signs in within a few seconds",
                device.name
            ));
            refreshed_card(&api, toast).await
        }
        Err(err) => keep_form(Toast::error(err)),
    }
}

pub async fn remove(State(api): State<ApiClient>, Path(id): Path<DeviceId>) -> Response {
    match api
        .send(Method::DELETE, &format!("/api/devices/{id}"))
        .await
    {
        Ok(()) => {
            refreshed_card(
                &api,
                Toast::success("Device removed; its key no longer works"),
            )
            .await
        }
        Err(err) => keep_form(Toast::error(err)),
    }
}

async fn refreshed_card(api: &ApiClient, toast: Toast) -> Response {
    match fetch(api).await {
        Ok(devices) => render_all(&[&DevicesCard::new(devices), &toast]),
        Err(err) => render(Toast::error(err)),
    }
}

/// `2026-11-12T08:30:45Z` → `2026-11-12 08:30 UTC`.
fn readable_time(rfc3339: &str) -> String {
    match rfc3339.get(..16) {
        Some(minutes) => format!("{} UTC", minutes.replace('T', " ")),
        None => rfc3339.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_times_to_the_minute() {
        assert_eq!(
            readable_time("2026-11-12T08:30:45Z"),
            "2026-11-12 08:30 UTC"
        );
    }
}
