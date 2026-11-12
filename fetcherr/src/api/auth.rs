//! API key check for every `/api` route (the WebSocket included). `/health`, the docs, the UI
//! pages and the two pairing endpoints a device uses before it has a key stay open. Browsers
//! cannot set WebSocket headers, so `?api_key=` works too. Paired devices use their own keys.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

use crate::devices::Devices;
use crate::error::AppError;
use crate::settings::SettingsStore;

const API_KEY_HEADER: &str = "x-api-key";
const API_KEY_PARAM: &str = "api_key";
/// `POST /api/auth/pair` and `GET /api/auth/pair/{secret}`.
const PAIRING_PATH: &str = "/api/auth/pair";

/// Marks a request made in-process by the built-in UI ([`crate::ui::ApiClient`]). Extensions
/// cannot come from the network, so this cannot be forged.
#[derive(Debug, Clone, Copy)]
pub struct InternalCall;

#[derive(Clone)]
pub struct AuthState {
    pub settings: Arc<SettingsStore>,
    pub devices: Devices,
}

pub async fn require_api_key(
    State(auth): State<AuthState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let open = !path.starts_with("/api/") || is_pairing(path);
    let internal = request.extensions().get::<InternalCall>().is_some();
    if open || internal || is_authorized(&request, &auth) {
        return next.run(request).await;
    }
    AppError::Unauthorized.into_response()
}

fn is_pairing(path: &str) -> bool {
    path.strip_prefix(PAIRING_PATH)
        .is_some_and(|rest| rest.is_empty() || (rest.starts_with('/') && !rest[1..].contains('/')))
}

fn is_authorized(request: &Request, auth: &AuthState) -> bool {
    let main_key = &auth.settings.get().api.key;
    presented_keys(request)
        .into_iter()
        .any(|key| matches(key, main_key) || auth.devices.authenticate(key).is_some())
}

fn presented_keys(request: &Request) -> Vec<&str> {
    let from_header = request
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|value| value.to_str().ok());
    let from_query = request.uri().query().and_then(query_key);
    [from_header, from_query]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .collect()
}

fn matches(presented: &str, expected: &str) -> bool {
    !expected.is_empty() && bool::from(presented.as_bytes().ct_eq(expected.as_bytes()))
}

/// The raw `api_key` value; keys are hex, so no percent-decoding is needed.
fn query_key(query: &str) -> Option<&str> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == API_KEY_PARAM)
        .map(|(_, value)| value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn request(uri: &str, header: Option<&str>) -> Request {
        let mut builder = axum::http::Request::get(uri);
        if let Some(key) = header {
            builder = builder.header(API_KEY_HEADER, key);
        }
        builder.body(Body::empty()).unwrap()
    }

    fn carries_key(request: &Request, expected: &str) -> bool {
        presented_keys(request)
            .into_iter()
            .any(|key| matches(key, expected))
    }

    #[test]
    fn leaves_only_the_device_pairing_endpoints_open() {
        assert!(is_pairing("/api/auth/pair"));
        assert!(is_pairing("/api/auth/pair/abc123"));
        assert!(!is_pairing("/api/auth/pair/abc/x"));
        assert!(!is_pairing("/api/auth/pairing"));
        assert!(!is_pairing("/api/devices/pair"));
    }

    #[test]
    fn accepts_the_key_in_a_header_or_the_query() {
        assert!(carries_key(&request("/api/x", Some("abc")), "abc"));
        assert!(carries_key(
            &request("/api/ws/events?requests=1&api_key=abc", None),
            "abc"
        ));
        assert!(!carries_key(
            &request("/api/x?api_key=abd", Some("abx")),
            "abc"
        ));
        assert!(!carries_key(&request("/api/x", None), "abc"));
        assert!(!carries_key(&request("/api/x", Some("")), ""));
    }
}
