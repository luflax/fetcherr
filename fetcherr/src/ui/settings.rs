use askama::Template;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::Form;
use serde::Deserialize;

use super::api_client::{ApiClient, ApiError};
use super::page::{BaseHref, Layout, Toast, render, render_all};
use crate::domain::Category;
use crate::settings::{
    DEFAULT_POLL_INTERVAL_MS, DownloadClientSettingsInput, DownloadClientSettingsView,
    DownloadClientTestResult, IndexerSettingsInput, IndexerSettingsView, IndexerTestResult,
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
    pub dom_id: &'static str,
    pub options: Vec<CategoryOption>,
    pub hint: Option<String>,
    pub out_of_band: bool,
}

impl CategoryChoices {
    fn new(dom_id: &'static str, categories: Vec<Category>, selected: &[u32]) -> Self {
        let options = categories
            .into_iter()
            .map(|category| CategoryOption {
                checked: selected.contains(&category.id),
                id: category.id,
                name: category.name,
            })
            .collect();
        Self {
            dom_id,
            options,
            hint: None,
            out_of_band: false,
        }
    }

    fn unavailable(dom_id: &'static str, err: ApiError) -> Self {
        Self {
            dom_id,
            options: Vec::new(),
            hint: Some(err.message),
            out_of_band: false,
        }
    }
}

pub async fn load_category_choices(
    api: &ApiClient,
    dom_id: &'static str,
    selected: &[u32],
    out_of_band: bool,
) -> CategoryChoices {
    let choices = match api.get::<Vec<Category>>("/api/indexer/categories").await {
        Ok(categories) => CategoryChoices::new(dom_id, categories, selected),
        Err(err) => CategoryChoices::unavailable(dom_id, err),
    };
    CategoryChoices {
        out_of_band,
        ..choices
    }
}

#[derive(Template)]
#[template(path = "partials/indexer_card.html")]
struct IndexerCard {
    settings: IndexerSettingsView,
    categories: CategoryChoices,
}

impl IndexerCard {
    async fn load(api: &ApiClient, settings: IndexerSettingsView) -> Self {
        let categories =
            load_category_choices(api, "indexer-categories", &settings.categories, false).await;
        Self {
            settings,
            categories,
        }
    }
}

#[derive(Template)]
#[template(path = "partials/download_client_card.html")]
struct DownloadClientCard {
    settings: DownloadClientSettingsView,
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPage {
    layout: Layout,
    indexer: IndexerCard,
    download_client: DownloadClientCard,
}

pub async fn page(State(api): State<ApiClient>, base: BaseHref) -> Response {
    let indexer = api.get::<IndexerSettingsView>("/api/settings/indexer");
    let download_client = api.get::<DownloadClientSettingsView>("/api/settings/download-client");
    let (indexer, download_client) = tokio::join!(indexer, download_client);
    let (indexer, download_client) = match (indexer, download_client) {
        (Ok(indexer), Ok(download_client)) => (indexer, download_client),
        (Err(err), _) | (_, Err(err)) => return render(Toast::error(err)),
    };
    render(SettingsPage {
        layout: Layout::new(base, "settings"),
        indexer: IndexerCard::load(&api, indexer).await,
        download_client: DownloadClientCard {
            settings: download_client,
        },
    })
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct IndexerForm {
    url: String,
    api_key: String,
    categories: Vec<u32>,
}

impl From<IndexerForm> for IndexerSettingsInput {
    fn from(form: IndexerForm) -> Self {
        Self {
            url: form.url,
            api_key: Some(form.api_key),
            categories: form.categories,
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
        Ok(saved) => {
            let card = IndexerCard::load(&api, saved).await;
            render_all(&[&card, &Toast::success("Prowlarr settings saved")])
        }
        Err(err) => keep_form(Toast::error(err)),
    }
}

pub async fn test_indexer(State(api): State<ApiClient>, Form(form): Form<IndexerForm>) -> Response {
    let input = IndexerSettingsInput::from(form);
    match api
        .post::<_, IndexerTestResult>("/api/settings/indexer/test", &input)
        .await
    {
        Ok(result) => {
            let badge = TestResult {
                dom_id: "indexer-test",
                ok: true,
                message: format!("Connected to Prowlarr {}", result.version),
            };
            let categories = CategoryChoices {
                out_of_band: true,
                ..CategoryChoices::new("indexer-categories", result.categories, &input.categories)
            };
            render_all(&[&badge, &categories])
        }
        Err(err) => render(TestResult {
            dom_id: "indexer-test",
            ok: false,
            message: err.message,
        }),
    }
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

/// Shows the toast but leaves the form untouched so the user can fix the input.
fn keep_form(toast: Toast) -> Response {
    let mut response = render(toast);
    response
        .headers_mut()
        .insert("HX-Reswap", axum::http::HeaderValue::from_static("none"));
    response.into_response()
}
