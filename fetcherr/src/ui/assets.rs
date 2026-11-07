use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

/// `static/` compiled into the binary (read from disk in debug builds).
#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

pub async fn serve(Path(path): Path<String>) -> Response {
    let Some(file) = Assets::get(&path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    (
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
        ],
        file.data,
    )
        .into_response()
}
