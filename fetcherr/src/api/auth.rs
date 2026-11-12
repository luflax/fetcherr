//! API key check for every `/api` route (the WebSocket included). `/health`, the docs and the
//! UI pages stay open. Browsers cannot set WebSocket headers, so `?api_key=` works too.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

use crate::error::AppError;
use crate::settings::SettingsStore;

const API_KEY_HEADER: &str = "x-api-key";
const API_KEY_PARAM: &str = "api_key";

/// Marks a request made in-process by the built-in UI ([`crate::ui::ApiClient`]). Extensions
/// cannot come from the network, so this cannot be forged.
#[derive(Debug, Clone, Copy)]
pub struct InternalCall;

pub async fn require_api_key(
    State(settings): State<Arc<SettingsStore>>,
    request: Request,
    next: Next,
) -> Response {
    let is_api = request.uri().path().starts_with("/api/");
    let internal = request.extensions().get::<InternalCall>().is_some();
    if !is_api || internal || carries_key(&request, &settings.get().api.key) {
        return next.run(request).await;
    }
    AppError::Unauthorized.into_response()
}

fn carries_key(request: &Request, expected: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    let from_header = request
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|value| value.to_str().ok());
    let from_query = request.uri().query().and_then(query_key);
    [from_header, from_query]
        .into_iter()
        .flatten()
        .any(|key| bool::from(key.trim().as_bytes().ct_eq(expected.as_bytes())))
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
