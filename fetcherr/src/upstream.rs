use std::fmt::Display;

/// A failure while talking to an external service (Prowlarr, qBittorrent, ...).
#[derive(Debug, Clone, thiserror::Error)]
#[error("{service}: {message}")]
pub struct UpstreamError {
    pub service: &'static str,
    pub message: String,
    /// The service answered that the requested item does not exist.
    pub not_found: bool,
}

impl UpstreamError {
    pub fn new(service: &'static str, message: impl Display) -> Self {
        Self {
            service,
            message: message.to_string(),
            not_found: false,
        }
    }

    pub fn not_found(service: &'static str, message: impl Display) -> Self {
        Self {
            not_found: true,
            ..Self::new(service, message)
        }
    }
}

/// Turns a non-success HTTP response into an [`UpstreamError`] carrying status and body.
pub async fn ensure_success(
    service: &'static str,
    response: reqwest::Response,
) -> Result<reqwest::Response, UpstreamError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    let detail = body.trim();
    let message = if detail.is_empty() {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status}: {}", truncate(detail, 200))
    };
    Err(UpstreamError::new(service, message))
}

fn truncate(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((index, _)) => &text[..index],
        None => text,
    }
}

pub fn parse_base_url(service: &'static str, raw: &str) -> Result<reqwest::Url, UpstreamError> {
    let trimmed = raw.trim().trim_end_matches('/');
    let url = reqwest::Url::parse(&format!("{trimmed}/"))
        .map_err(|err| UpstreamError::new(service, format!("invalid URL '{raw}': {err}")))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        scheme => Err(UpstreamError::new(
            service,
            format!("unsupported URL scheme '{scheme}'"),
        )),
    }
}
