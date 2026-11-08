use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::Form;
use serde::Deserialize;

use super::api_client::{ApiClient, ApiError};
use super::page::{BaseHref, Layout, Toast, render, render_all};
use crate::domain::{Category, MediaType};
use crate::quality::Quality;
use crate::settings::{
    DEFAULT_POLL_INTERVAL_MS, DownloadClientSettingsInput, DownloadClientSettingsView,
    DownloadClientTestResult, IndexerSettingsInput, IndexerSettingsView, IndexerTestResult,
    MediaTypeSettingsInput, MediaTypeSettingsView,
};

pub struct CategoryOption {
    pub id: u32,
    pub name: String,
    pub checked: bool,
}

/// Checkbox list of top-level indexer categories (Movies, TV, ...).
#[derive(Template)]
#[template(path = "partials/category_choices.html")]
pub struct CategoryChoices {
    pub dom_id: String,
    pub options: Vec<CategoryOption>,
    pub hint: Option<String>,
}

impl CategoryChoices {
    /// `categories` is the indexer's list, or why it could not be loaded.
    pub fn new(
        dom_id: String,
        categories: &Result<Vec<Category>, ApiError>,
        selected: &[u32],
    ) -> Self {
        match categories {
            Ok(categories) => Self {
                dom_id,
                options: categories
                    .iter()
                    .map(|category| CategoryOption {
                        checked: selected.contains(&category.id),
                        id: category.id,
                        name: category.name.clone(),
                    })
                    .collect(),
                hint: None,
            },
            Err(err) => Self {
                dom_id,
                options: Vec::new(),
                hint: Some(err.message.clone()),
            },
        }
    }
}

pub async fn fetch_categories(api: &ApiClient) -> Result<Vec<Category>, ApiError> {
    api.get::<Vec<Category>>("/api/indexer/categories").await
}

pub struct QualityChoice {
    pub id: &'static str,
    pub label: &'static str,
    pub checked: bool,
}

impl QualityChoice {
    /// Every quality: the `ranked` ones first and checked, in preference order, then the rest
    /// unchecked. An empty ranking allows everything, so all are checked, best first.
    pub fn ranked(ranked: &[Quality]) -> Vec<Self> {
        if ranked.is_empty() {
            return Quality::ALL.map(|quality| Self::new(quality, true)).into();
        }
        let rest = Quality::ALL
            .into_iter()
            .filter(|quality| !ranked.contains(quality));
        ranked
            .iter()
            .map(|quality| Self::new(*quality, true))
            .chain(rest.map(|quality| Self::new(quality, false)))
            .collect()
    }

    fn new(quality: Quality, checked: bool) -> Self {
        Self {
            id: quality.id(),
            label: quality.label(),
            checked,
        }
    }
}

#[derive(Template)]
#[template(path = "partials/indexer_card.html")]
struct IndexerCard {
    settings: IndexerSettingsView,
}

#[derive(Template)]
#[template(path = "partials/download_client_card.html")]
struct DownloadClientCard {
    settings: DownloadClientSettingsView,
}

#[derive(Template)]
#[template(path = "partials/media_type_card.html")]
struct MediaTypeCard {
    settings: MediaTypeSettingsView,
    categories: CategoryChoices,
    qualities: Vec<QualityChoice>,
}

impl MediaTypeCard {
    fn new(settings: MediaTypeSettingsView, categories: &Result<Vec<Category>, ApiError>) -> Self {
        let dom_id = format!("{}-categories", settings.media_type);
        Self {
            categories: CategoryChoices::new(dom_id, categories, &settings.categories),
            qualities: QualityChoice::ranked(&settings.allowed_qualities),
            settings,
        }
    }
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPage {
    layout: Layout,
    indexer: IndexerCard,
    download_client: DownloadClientCard,
    media_types: Vec<MediaTypeCard>,
}

pub async fn page(State(api): State<ApiClient>, base: BaseHref) -> Response {
    let (indexer, download_client, media_types, categories) = tokio::join!(
        api.get::<IndexerSettingsView>("/api/settings/indexer"),
        api.get::<DownloadClientSettingsView>("/api/settings/download-client"),
        api.get::<Vec<MediaTypeSettingsView>>("/api/settings/media-types"),
        fetch_categories(&api),
    );
    let (indexer, download_client, media_types) = match (indexer, download_client, media_types) {
        (Ok(indexer), Ok(download_client), Ok(media_types)) => {
            (indexer, download_client, media_types)
        }
        (Err(err), _, _) | (_, Err(err), _) | (_, _, Err(err)) => return render(Toast::error(err)),
    };
    render(SettingsPage {
        layout: Layout::new(base, "settings"),
        indexer: IndexerCard { settings: indexer },
        download_client: DownloadClientCard {
            settings: download_client,
        },
        media_types: media_types
            .into_iter()
            .map(|settings| MediaTypeCard::new(settings, &categories))
            .collect(),
    })
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct IndexerForm {
    url: String,
    api_key: String,
}

impl From<IndexerForm> for IndexerSettingsInput {
    fn from(form: IndexerForm) -> Self {
        Self {
            url: form.url,
            api_key: Some(form.api_key),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct DownloadClientForm {
    url: String,
    username: String,
    password: String,
    root_folder: String,
    poll_interval_ms: String,
}

impl From<DownloadClientForm> for DownloadClientSettingsInput {
    fn from(form: DownloadClientForm) -> Self {
        Self {
            url: form.url,
            username: form.username,
            password: Some(form.password),
            root_folder: form.root_folder,
            poll_interval_ms: form
                .poll_interval_ms
                .trim()
                .parse()
                .unwrap_or(DEFAULT_POLL_INTERVAL_MS),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct MediaTypeForm {
    categories: Vec<u32>,
    download_folder: String,
    /// Ticked qualities in list order, which is the preference order.
    allowed_qualities: Vec<String>,
}

impl From<MediaTypeForm> for MediaTypeSettingsInput {
    fn from(form: MediaTypeForm) -> Self {
        Self {
            categories: form.categories,
            download_folder: form.download_folder,
            allowed_qualities: form
                .allowed_qualities
                .iter()
                .filter_map(|id| Quality::from_id(id))
                .collect(),
        }
    }
}

#[derive(Template)]
#[template(path = "partials/test_result.html")]
struct TestResult {
    dom_id: &'static str,
    ok: bool,
    message: String,
}

pub async fn save_indexer(State(api): State<ApiClient>, Form(form): Form<IndexerForm>) -> Response {
    let input = IndexerSettingsInput::from(form);
    match api
        .put::<_, IndexerSettingsView>("/api/settings/indexer", &input)
        .await
    {
        Ok(settings) => render_all(&[
            &IndexerCard { settings },
            &Toast::success("Prowlarr settings saved"),
        ]),
        Err(err) => keep_form(Toast::error(err)),
    }
}

pub async fn test_indexer(State(api): State<ApiClient>, Form(form): Form<IndexerForm>) -> Response {
    let input = IndexerSettingsInput::from(form);
    let outcome = api
        .post::<_, IndexerTestResult>("/api/settings/indexer/test", &input)
        .await;
    render(TestResult {
        dom_id: "indexer-test",
        ok: outcome.is_ok(),
        message: match outcome {
            Ok(result) => format!("Connected to Prowlarr {}", result.version),
            Err(err) => err.message,
        },
    })
}

pub async fn save_download_client(
    State(api): State<ApiClient>,
    Form(form): Form<DownloadClientForm>,
) -> Response {
    let input = DownloadClientSettingsInput::from(form);
    let saved = api
        .put::<_, DownloadClientSettingsView>("/api/settings/download-client", &input)
        .await;
    match saved {
        Ok(settings) => render_all(&[
            &DownloadClientCard { settings },
            &Toast::success("qBittorrent settings saved"),
        ]),
        Err(err) => keep_form(Toast::error(err)),
    }
}

pub async fn test_download_client(
    State(api): State<ApiClient>,
    Form(form): Form<DownloadClientForm>,
) -> Response {
    let input = DownloadClientSettingsInput::from(form);
    let outcome = api
        .post::<_, DownloadClientTestResult>("/api/settings/download-client/test", &input)
        .await;
    render(TestResult {
        dom_id: "download-client-test",
        ok: outcome.is_ok(),
        message: match outcome {
            Ok(result) => format!("Connected to qBittorrent {}", result.version),
            Err(err) => err.message,
        },
    })
}

pub async fn save_media_type(
    State(api): State<ApiClient>,
    Path(media_type): Path<String>,
    Form(form): Form<MediaTypeForm>,
) -> Response {
    let Some(media_type) = MediaType::from_id(&media_type) else {
        return keep_form(Toast::error("unknown media type"));
    };
    let input = MediaTypeSettingsInput::from(form);
    let uri = format!("/api/settings/media-types/{media_type}");
    match api.put::<_, MediaTypeSettingsView>(&uri, &input).await {
        Ok(settings) => {
            let toast = Toast::success(format!("{} settings saved", settings.label));
            let card = MediaTypeCard::new(settings, &fetch_categories(&api).await);
            render_all(&[&card, &toast])
        }
        Err(err) => keep_form(Toast::error(err)),
    }
}

/// Shows the toast but leaves the form untouched so the user can fix the input.
fn keep_form(toast: Toast) -> Response {
    let mut response = render(toast);
    response
        .headers_mut()
        .insert("HX-Reswap", axum::http::HeaderValue::from_static("none"));
    response.into_response()
}
