//! The UI's only way to read data or act: real HTTP requests through the API router,
//! in-process. Every page render exercises the public API contract.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tower::ServiceExt;

use crate::error::ErrorBody;

const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub struct ApiClient {
    api: Router,
}

#[derive(Debug)]
pub struct ApiError {
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl ApiClient {
    pub fn new(api: Router) -> Self {
        Self { api }
    }

    pub async fn get<T: DeserializeOwned>(&self, uri: &str) -> Result<T, ApiError> {
        self.call(Method::GET, uri, Body::empty()).await
    }

    pub async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        uri: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        self.call(Method::POST, uri, json_body(body)?).await
    }

    pub async fn put<B: Serialize, T: DeserializeOwned>(
        &self,
        uri: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        self.call(Method::PUT, uri, json_body(body)?).await
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: Method,
        uri: &str,
        body: Body,
    ) -> Result<T, ApiError> {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .map_err(|err| internal(format!("invalid API request {uri}: {err}")))?;
        let response = match self.api.clone().oneshot(request).await {
            Ok(response) => response,
            Err(never) => match never {},
        };

        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), MAX_RESPONSE_BYTES)
            .await
            .map_err(|err| internal(format!("cannot read API response: {err}")))?;
        if !status.is_success() {
            return Err(ApiError {
                message: error_message(&bytes, status),
            });
        }
        serde_json::from_slice(&bytes)
            .map_err(|err| internal(format!("unexpected API response from {uri}: {err}")))
    }
}

fn json_body<B: Serialize>(body: &B) -> Result<Body, ApiError> {
    serde_json::to_vec(body)
        .map(Body::from)
        .map_err(|err| internal(format!("cannot encode API request: {err}")))
}

fn error_message(bytes: &[u8], status: StatusCode) -> String {
    serde_json::from_slice::<ErrorBody>(bytes)
        .map(|body| body.error.message)
        .unwrap_or_else(|_| {
            let text = String::from_utf8_lossy(bytes);
            if text.trim().is_empty() {
                status.to_string()
            } else {
                text.trim().to_string()
            }
        })
}

fn internal(message: String) -> ApiError {
    ApiError { message }
}
