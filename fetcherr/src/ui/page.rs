//! Shared page plumbing: layout data, Ingress base path, rendering and toasts.

use askama::Template;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{Html, IntoResponse, Response};

use super::api_client::ApiClient;
use crate::settings::ApiKeyView;

/// Header set by the Home Assistant Ingress proxy, e.g. `/api/hassio_ingress/<token>`.
const INGRESS_PATH_HEADER: &str = "x-ingress-path";

pub struct Layout {
    /// `<base href>`: every UI, API and WebSocket URL is relative to it.
    pub base_href: String,
    pub active: &'static str,
    pub version: &'static str,
    /// For the browser's own API calls (WebSocket, layout editor).
    pub api_key: String,
}

/// Extracts the `<base href>` for a page, honouring the Ingress prefix.
pub struct BaseHref(pub String);

impl<S: Send + Sync> FromRequestParts<S> for BaseHref {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let prefix = parts
            .headers
            .get(INGRESS_PATH_HEADER)
            .and_then(|value| value.to_str().ok())
            .filter(|path| is_safe_path(path))
            .unwrap_or("");
        Ok(Self(format!("{}/", prefix.trim_end_matches('/'))))
    }
}

fn is_safe_path(path: &str) -> bool {
    path.starts_with('/')
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
}

impl Layout {
    pub async fn load(api: &ApiClient, base: BaseHref, active: &'static str) -> Self {
        let api_key = api
            .get::<ApiKeyView>("/api/settings/api-key")
            .await
            .map(|view| view.key)
            .unwrap_or_default();
        Self {
            base_href: base.0,
            active,
            version: env!("CARGO_PKG_VERSION"),
            api_key,
        }
    }
}

pub fn render(template: impl Template) -> Response {
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            tracing::error!("template error: {err}");
            (StatusCode::INTERNAL_SERVER_ERROR, "template error").into_response()
        }
    }
}

/// Fragment appended to `#toasts` out-of-band, next to whatever the request swaps.
#[derive(Template)]
#[template(path = "partials/toast.html")]
pub struct Toast {
    pub kind: &'static str,
    pub message: String,
}

impl Toast {
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            kind: "success",
            message: message.into(),
        }
    }

    pub fn error(message: impl std::fmt::Display) -> Self {
        Self {
            kind: "error",
            message: message.to_string(),
        }
    }
}

/// Renders several fragments into one htmx response (main swap + out-of-band parts).
pub fn render_all(parts: &[&dyn askama::DynTemplate]) -> Response {
    let mut html = String::new();
    for part in parts {
        match part.dyn_render() {
            Ok(rendered) => html.push_str(&rendered),
            Err(err) => {
                tracing::error!("template error: {err}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "template error").into_response();
            }
        }
    }
    Html(html).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ingress_paths_only_when_safe() {
        assert!(is_safe_path("/api/hassio_ingress/abc-DEF_123"));
        assert!(!is_safe_path("javascript:alert(1)"));
        assert!(!is_safe_path("/x\"><script>"));
    }
}
