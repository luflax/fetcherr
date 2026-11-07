use askama::Template;
use axum::extract::State;
use axum::response::Response;
use axum_extra::extract::Form;
use serde::Deserialize;

use super::api_client::ApiClient;
use super::format;
use super::page::{BaseHref, Layout, Toast, render, render_all};
use crate::domain::{Download, DownloadStarted, DownloadState, ReleaseId, StartDownload};

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
        layout: Layout::new(base, "downloads"),
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
