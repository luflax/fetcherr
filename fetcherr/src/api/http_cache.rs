//! `ETag` + `Cache-Control` for responses clients poll (layout and lists), answering
//! `304 Not Modified` when the client already has the same body.

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use sha1::{Digest, Sha1};

use crate::error::AppError;

/// Seconds a client may reuse a layout or list response without asking again.
pub const MAX_AGE_SECS: u32 = 60;

pub fn cached_json<T: Serialize>(
    request_headers: &HeaderMap,
    body: &T,
) -> Result<Response, AppError> {
    let bytes = serde_json::to_vec(body)
        .map_err(|err| AppError::Internal(format!("cannot encode response: {err}")))?;
    // Weak: the body may be served gzip-compressed, which strong ETags would have to reflect.
    let etag = format!("W/\"{}\"", hex::encode(&Sha1::digest(&bytes)[..16]));
    let headers = [
        (header::ETAG, header_value(&etag)?),
        (
            header::CACHE_CONTROL,
            header_value(&format!("private, max-age={MAX_AGE_SECS}"))?,
        ),
    ];
    if matches_etag(request_headers, &etag) {
        return Ok((StatusCode::NOT_MODIFIED, headers).into_response());
    }
    let content_type = (
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok((headers, [content_type], bytes).into_response())
}

fn header_value(value: &str) -> Result<HeaderValue, AppError> {
    HeaderValue::from_str(value).map_err(|err| AppError::Internal(err.to_string()))
}

/// `If-None-Match` lists ETags (maybe weak, `W/"..."`) or is `*`.
fn matches_etag(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|candidate| candidate.trim().trim_start_matches("W/"))
        .any(|candidate| candidate == etag.trim_start_matches("W/") || candidate == "*")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_304_for_a_known_etag() {
        let fresh = cached_json(&HeaderMap::new(), &vec![1, 2]).unwrap();
        assert_eq!(fresh.status(), StatusCode::OK);
        let etag = fresh.headers()[header::ETAG].clone();
        assert_eq!(
            fresh.headers()[header::CACHE_CONTROL],
            "private, max-age=60"
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_str(&format!("\"x\", {}", etag.to_str().unwrap())).unwrap(),
        );
        assert_eq!(
            cached_json(&headers, &vec![1, 2]).unwrap().status(),
            StatusCode::NOT_MODIFIED
        );
        assert_eq!(
            cached_json(&headers, &vec![3]).unwrap().status(),
            StatusCode::OK
        );
    }
}
