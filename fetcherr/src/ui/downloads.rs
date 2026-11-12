use askama::Template;
use axum::extract::{Path, State};
use axum::http::Method;
use axum::response::Response;
use axum_extra::extract::Form;
use serde::Deserialize;

use super::api_client::ApiClient;
use super::format;
use super::page::{BaseHref, Layout, Toast, render, render_all};
use crate::domain::{
    Download, DownloadId, DownloadStarted, DownloadState, MediaType, ReleaseId, StartDownload,
};

pub struct DownloadRow {
    pub id: String,
    pub name: String,
    pub state: &'static str,
    pub state_label: &'static str,
    pub percent: String,
    pub size: String,
    pub speed: String,
    pub eta: String,
    pub seeds: u32,
    pub peers: u32,
    pub stopped: bool,
    pub media_type: &'static str,
}

impl From<Download> for DownloadRow {
    fn from(download: Download) -> Self {
        Self {
            id: download.id.to_string(),
            name: download.name,
            state: state_class(download.state),
            state_label: download.state.label(),
            percent: format::percent(download.progress),
            size: format::bytes(download.size),
            speed: format::speed(download.dl_speed),
            eta: format::eta(download.eta_secs),
            seeds: download.seeds,
            peers: download.peers,
            stopped: download.state.is_stopped(),
            media_type: download.media_type.map_or("—", MediaType::label),
        }
    }
}

/// CSS modifier of the state pill (`downloads.js` uses the same grouping).
fn state_class(state: DownloadState) -> &'static str {
    use DownloadState::*;
    match state {
        Downloading | FetchingMetadata => "active",
        Seeding | Completed => "done",
        Error => "error",
        Queued | Stalled | Paused | Checking | Moving | Unknown => "idle",
    }
}

#[derive(Template)]
#[template(path = "partials/download_rows.html")]
struct DownloadRows {
    rows: Vec<DownloadRow>,
    error: Option<String>,
}

impl DownloadRows {
    async fn load(api: &ApiClient) -> Self {
        match api.get::<Vec<Download>>("/api/downloads").await {
            Ok(downloads) => Self {
                rows: downloads.into_iter().map(DownloadRow::from).collect(),
                error: None,
            },
            Err(err) => Self {
                rows: Vec::new(),
                error: Some(err.message),
            },
        }
    }
}

#[derive(Template)]
#[template(path = "downloads.html")]
struct DownloadsPage {
    layout: Layout,
    rows: DownloadRows,
}

pub async fn page(State(api): State<ApiClient>, base: BaseHref) -> Response {
    render(DownloadsPage {
        layout: Layout::load(&api, base, "downloads").await,
        rows: DownloadRows::load(&api).await,
    })
}

pub async fn rows(State(api): State<ApiClient>) -> Response {
    render(DownloadRows::load(&api).await)
}

#[derive(Template)]
#[template(path = "partials/download_button.html")]
pub struct DownloadButton {
    pub release_id: String,
    pub added: bool,
}

#[derive(Deserialize)]
pub struct StartForm {
    release_id: ReleaseId,
}

pub async fn start(State(api): State<ApiClient>, Form(form): Form<StartForm>) -> Response {
    let request = StartDownload {
        release_id: form.release_id.clone(),
        media_type: None,
        save_path: None,
    };
    let outcome = api
        .post::<_, DownloadStarted>("/api/downloads", &request)
        .await;
    let button = DownloadButton {
        release_id: form.release_id.to_string(),
        added: outcome.is_ok(),
    };
    match outcome {
        Ok(_) => render_all(&[&button, &Toast::success("Download started")]),
        Err(err) => render_all(&[&button, &Toast::error(err)]),
    }
}

pub async fn pause(State(api): State<ApiClient>, Path(id): Path<String>) -> Response {
    act_on(&api, Method::POST, &id, "/pause", "Download paused").await
}

pub async fn resume(State(api): State<ApiClient>, Path(id): Path<String>) -> Response {
    act_on(&api, Method::POST, &id, "/resume", "Download resumed").await
}

#[derive(Deserialize)]
pub struct DeleteForm {
    id: String,
    /// Checkbox: present only when ticked.
    delete_files: Option<String>,
}

pub async fn delete(State(api): State<ApiClient>, Form(form): Form<DeleteForm>) -> Response {
    let query = format!("?delete_files={}", form.delete_files.is_some());
    act_on(&api, Method::DELETE, &form.id, &query, "Download deleted").await
}

/// Calls `/api/downloads/{id}{suffix}` and answers with a toast; rows update over the WebSocket.
async fn act_on(api: &ApiClient, method: Method, id: &str, suffix: &str, done: &str) -> Response {
    let toast = match DownloadId::try_from(id.to_string()) {
        Ok(id) => match api
            .send(method, &format!("/api/downloads/{id}{suffix}"))
            .await
        {
            Ok(()) => Toast::success(done),
            Err(err) => Toast::error(err),
        },
        Err(err) => Toast::error(err),
    };
    render(toast)
}
