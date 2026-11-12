//! Settings cards for TMDB, OMDb, Jellyfin, the library and the API key.

use askama::Template;
use axum::extract::State;
use axum::response::Response;
use axum_extra::extract::Form;
use serde::Deserialize;

use super::api_client::ApiClient;
use super::page::{Toast, render, render_all};
use super::settings::{TestResult, keep_form};
use crate::settings::{
    ApiKeyView, ImportMode, JellyfinSettingsInput, JellyfinSettingsView, LibrarySettings,
    OmdbSettingsInput, OmdbSettingsView, PathMapping, TmdbSettingsInput, TmdbSettingsView,
    VersionTestResult,
};

#[derive(Template)]
#[template(path = "partials/tmdb_card.html")]
pub struct TmdbCard {
    pub settings: TmdbSettingsView,
}

#[derive(Template)]
#[template(path = "partials/omdb_card.html")]
pub struct OmdbCard {
    pub settings: OmdbSettingsView,
}

#[derive(Template)]
#[template(path = "partials/jellyfin_card.html")]
pub struct JellyfinCard {
    pub settings: JellyfinSettingsView,
}

#[derive(Template)]
#[template(path = "partials/library_card.html")]
pub struct LibraryCard {
    settings: LibrarySettings,
    hardlink: bool,
    /// `remote = local`, one per line.
    mappings: String,
}

impl LibraryCard {
    pub fn new(settings: LibrarySettings) -> Self {
        let mappings = settings
            .path_mappings
            .iter()
            .map(|mapping| format!("{} = {}", mapping.remote, mapping.local))
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            hardlink: settings.import_mode == ImportMode::Hardlink,
            mappings,
            settings,
        }
    }
}

#[derive(Template)]
#[template(path = "partials/api_key_card.html")]
pub struct ApiKeyCard {
    pub key: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct TmdbForm {
    url: String,
    api_key: String,
    language: String,
    region: String,
}

impl From<TmdbForm> for TmdbSettingsInput {
    fn from(form: TmdbForm) -> Self {
        Self {
            url: form.url,
            api_key: Some(form.api_key),
            language: form.language,
            region: form.region,
        }
    }
}

pub async fn save_tmdb(State(api): State<ApiClient>, Form(form): Form<TmdbForm>) -> Response {
    let input = TmdbSettingsInput::from(form);
    match api
        .put::<_, TmdbSettingsView>("/api/settings/tmdb", &input)
        .await
    {
        Ok(settings) => render_all(&[
            &TmdbCard { settings },
            &Toast::success("TMDB settings saved"),
        ]),
        Err(err) => keep_form(Toast::error(err)),
    }
}

pub async fn test_tmdb(State(api): State<ApiClient>, Form(form): Form<TmdbForm>) -> Response {
    let input = TmdbSettingsInput::from(form);
    let outcome = api.post_no_content("/api/settings/tmdb/test", &input).await;
    render(TestResult {
        dom_id: "tmdb-test",
        ok: outcome.is_ok(),
        message: match outcome {
            Ok(()) => "Connected to TMDB".into(),
            Err(err) => err.message,
        },
    })
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct OmdbForm {
    url: String,
    api_key: String,
}

pub async fn save_omdb(State(api): State<ApiClient>, Form(form): Form<OmdbForm>) -> Response {
    let input = OmdbSettingsInput {
        url: form.url,
        api_key: Some(form.api_key),
    };
    match api
        .put::<_, OmdbSettingsView>("/api/settings/omdb", &input)
        .await
    {
        Ok(settings) => render_all(&[
            &OmdbCard { settings },
            &Toast::success("OMDb settings saved"),
        ]),
        Err(err) => keep_form(Toast::error(err)),
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct JellyfinForm {
    url: String,
    api_key: String,
    sync_interval_minutes: String,
}

impl From<JellyfinForm> for JellyfinSettingsInput {
    fn from(form: JellyfinForm) -> Self {
        Self {
            url: form.url,
            api_key: Some(form.api_key),
            sync_interval_minutes: form
                .sync_interval_minutes
                .trim()
                .parse()
                .unwrap_or(crate::settings::DEFAULT_SYNC_MINUTES),
        }
    }
}

pub async fn save_jellyfin(
    State(api): State<ApiClient>,
    Form(form): Form<JellyfinForm>,
) -> Response {
    let input = JellyfinSettingsInput::from(form);
    match api
        .put::<_, JellyfinSettingsView>("/api/settings/jellyfin", &input)
        .await
    {
        Ok(settings) => render_all(&[
            &JellyfinCard { settings },
            &Toast::success("Jellyfin settings saved"),
        ]),
        Err(err) => keep_form(Toast::error(err)),
    }
}

pub async fn test_jellyfin(
    State(api): State<ApiClient>,
    Form(form): Form<JellyfinForm>,
) -> Response {
    let input = JellyfinSettingsInput::from(form);
    let outcome = api
        .post::<_, VersionTestResult>("/api/settings/jellyfin/test", &input)
        .await;
    render(TestResult {
        dom_id: "jellyfin-test",
        ok: outcome.is_ok(),
        message: match outcome {
            Ok(result) => format!("Connected to {}", result.version),
            Err(err) => err.message,
        },
    })
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct LibraryForm {
    movies_folder: String,
    series_folder: String,
    import_mode: String,
    path_mappings: String,
    movie_folder: String,
    movie_file: String,
    series_folder_name: String,
    season_folder: String,
    episode_file: String,
}

impl LibraryForm {
    fn into_settings(self) -> Result<LibrarySettings, String> {
        let path_mappings = self
            .path_mappings
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let (remote, local) = line
                    .split_once('=')
                    .ok_or_else(|| format!("path mapping '{}' needs '='", line.trim()))?;
                Ok(PathMapping {
                    remote: remote.trim().into(),
                    local: local.trim().into(),
                })
            })
            .collect::<Result<_, String>>()?;
        let mut settings = LibrarySettings {
            movies_folder: self.movies_folder,
            series_folder: self.series_folder,
            import_mode: if self.import_mode == "move" {
                ImportMode::Move
            } else {
                ImportMode::Hardlink
            },
            path_mappings,
            ..Default::default()
        };
        settings.naming.movie_folder = self.movie_folder;
        settings.naming.movie_file = self.movie_file;
        settings.naming.series_folder = self.series_folder_name;
        settings.naming.season_folder = self.season_folder;
        settings.naming.episode_file = self.episode_file;
        Ok(settings)
    }
}

pub async fn save_library(State(api): State<ApiClient>, Form(form): Form<LibraryForm>) -> Response {
    let input = match form.into_settings() {
        Ok(input) => input,
        Err(message) => return keep_form(Toast::error(message)),
    };
    match api
        .put::<_, LibrarySettings>("/api/settings/library", &input)
        .await
    {
        Ok(settings) => render_all(&[
            &LibraryCard::new(settings),
            &Toast::success("Library settings saved"),
        ]),
        Err(err) => keep_form(Toast::error(err)),
    }
}

pub async fn regenerate_api_key(State(api): State<ApiClient>) -> Response {
    match api
        .post_empty::<ApiKeyView>("/api/settings/api-key/regenerate")
        .await
    {
        Ok(view) => render_all(&[
            &ApiKeyCard { key: view.key },
            &Toast::success("New API key saved. Reload open pages and update your clients."),
        ]),
        Err(err) => keep_form(Toast::error(err)),
    }
}
