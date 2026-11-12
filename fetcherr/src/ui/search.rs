use askama::Template;
use axum::extract::State;
use axum::response::Response;
use axum_extra::extract::Query;
use serde::{Deserialize, Serialize};

use super::api_client::ApiClient;
use super::format;
use super::page::{BaseHref, Layout, render};
use super::settings::{CategoryChoices, QualityChoice, fetch_categories};
use crate::domain::{MediaType, Release, SearchQuery, SortBy};
use crate::quality::Quality;
use crate::settings::MediaTypeSettingsView;

/// Search form exactly as the browser submits it; numbers stay text so blanks are fine.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SearchForm {
    pub query: String,
    /// Empty searches every media type.
    pub media_type: String,
    pub categories: Vec<u32>,
    pub qualities: Vec<String>,
    pub min_seeders: String,
    pub min_size_gb: String,
    pub max_size_gb: String,
    pub sort: String,
}

impl SearchForm {
    fn media_type(&self) -> Option<MediaType> {
        MediaType::from_id(&self.media_type)
    }

    fn to_query(&self) -> SearchQuery {
        SearchQuery {
            query: self.query.trim().to_string(),
            media_type: self.media_type(),
            categories: self.categories.clone(),
            qualities: self
                .qualities
                .iter()
                .filter_map(|id| Quality::from_id(id))
                .collect(),
            min_seeders: self.min_seeders.trim().parse().ok(),
            min_size: parse_gigabytes(&self.min_size_gb),
            max_size: parse_gigabytes(&self.max_size_gb),
            sort: match self.sort.as_str() {
                "size" => SortBy::Size,
                "age" => SortBy::Age,
                _ => SortBy::Seeders,
            },
            ..Default::default()
        }
    }
}

fn parse_gigabytes(raw: &str) -> Option<u64> {
    raw.trim()
        .parse::<f64>()
        .ok()
        .filter(|gb| *gb > 0.0)
        .map(format::gigabytes_to_bytes)
}

pub struct MediaTypeOption {
    pub id: &'static str,
    pub label: &'static str,
}

fn media_type_options() -> Vec<MediaTypeOption> {
    let all = MediaTypeOption {
        id: "",
        label: "All",
    };
    let each = MediaType::ALL
        .into_iter()
        .map(|media_type| MediaTypeOption {
            id: media_type.id(),
            label: media_type.label(),
        });
    std::iter::once(all).chain(each).collect()
}

/// Category and quality filters, preselected from the chosen media type's settings.
#[derive(Template)]
#[template(path = "partials/search_filters.html")]
struct SearchFilters {
    categories: CategoryChoices,
    /// In preference order (the form submits them in this order, which ranks the results).
    /// Absent for "All" and Software: each release then uses its media type's qualities.
    qualities: Option<Vec<QualityChoice>>,
}

impl SearchFilters {
    async fn load(api: &ApiClient, form: &SearchForm) -> Self {
        let (media_types, categories) = tokio::join!(
            api.get::<Vec<MediaTypeSettingsView>>("/api/settings/media-types"),
            fetch_categories(api),
        );
        let media_types = media_types.unwrap_or_default();
        let chosen = form
            .media_type()
            .and_then(|chosen| media_types.iter().find(|view| view.media_type == chosen));

        let selected_categories = if form.categories.is_empty() {
            default_categories(&media_types, chosen)
        } else {
            form.categories.clone()
        };
        let qualities = chosen.filter(|view| view.has_qualities).map(|view| {
            if form.qualities.is_empty() {
                QualityChoice::ranked(&view.allowed_qualities)
            } else {
                QualityChoice::ranked(&form.to_query().qualities)
            }
        });
        Self {
            categories: CategoryChoices::new(
                "search-categories".into(),
                &categories,
                &selected_categories,
            ),
            qualities,
        }
    }
}

/// The chosen media type's categories, or those of every media type.
fn default_categories(
    media_types: &[MediaTypeSettingsView],
    chosen: Option<&MediaTypeSettingsView>,
) -> Vec<u32> {
    match chosen {
        Some(view) => view.categories.clone(),
        None => media_types
            .iter()
            .flat_map(|view| view.categories.iter().copied())
            .collect(),
    }
}

#[derive(Template)]
#[template(path = "search.html")]
struct SearchPage {
    layout: Layout,
    form: SearchForm,
    media_types: Vec<MediaTypeOption>,
    filters: SearchFilters,
}

pub async fn page(
    State(api): State<ApiClient>,
    base: BaseHref,
    Query(form): Query<SearchForm>,
) -> Response {
    let filters = SearchFilters::load(&api, &form).await;
    render(SearchPage {
        layout: Layout::load(&api, base, "search").await,
        form,
        media_types: media_type_options(),
        filters,
    })
}

pub async fn filters(State(api): State<ApiClient>, Query(form): Query<SearchForm>) -> Response {
    render(SearchFilters::load(&api, &form).await)
}

pub struct ReleaseRow {
    pub release_id: String,
    pub title: String,
    pub info_url: Option<String>,
    pub quality: &'static str,
    pub media_type: &'static str,
    pub indexer: String,
    pub age: String,
    pub size: String,
    pub seeders: u32,
    pub leechers: u32,
}

impl From<Release> for ReleaseRow {
    fn from(release: Release) -> Self {
        let has_qualities = release.media_type.is_none_or(MediaType::has_qualities);
        Self {
            release_id: release.release_id.to_string(),
            title: release.title,
            info_url: release.info_url.filter(|url| is_web_link(url)),
            quality: if has_qualities {
                release.quality.label()
            } else {
                ""
            },
            media_type: release.media_type.map_or("", MediaType::label),
            indexer: release.indexer,
            age: format::age(release.age_hours),
            size: format::bytes(release.size),
            seeders: release.seeders,
            leechers: release.leechers,
        }
    }
}

/// Indexer links end up in `href`; anything but http(s) (e.g. `javascript:`) is dropped.
fn is_web_link(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

#[derive(Template)]
#[template(path = "partials/search_results.html")]
struct SearchResults {
    rows: Vec<ReleaseRow>,
    error: Option<String>,
}

pub async fn results(State(api): State<ApiClient>, Query(form): Query<SearchForm>) -> Response {
    let query = serde_html_form::to_string(form.to_query()).unwrap_or_default();
    let outcome = api
        .get::<Vec<Release>>(&format!("/api/search?{query}"))
        .await;
    render(match outcome {
        Ok(releases) => SearchResults {
            rows: releases.into_iter().map(ReleaseRow::from).collect(),
            error: None,
        },
        Err(err) => SearchResults {
            rows: Vec::new(),
            error: Some(err.message),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_numbers_become_absent_filters() {
        let form = SearchForm {
            query: " matrix ".into(),
            min_size_gb: "1.5".into(),
            sort: "age".into(),
            ..Default::default()
        };
        let query = form.to_query();
        assert_eq!(query.query, "matrix");
        assert_eq!(query.media_type, None);
        assert_eq!(query.min_seeders, None);
        assert_eq!(query.min_size, Some(1_610_612_736));
        assert_eq!(query.max_size, None);
        assert_eq!(query.sort, SortBy::Age);
    }

    #[test]
    fn serializes_media_type_and_repeated_filters_for_the_api() {
        let form = SearchForm {
            query: "x".into(),
            media_type: "movies".into(),
            categories: vec![2000, 5000],
            qualities: vec!["web_1080p".into(), "bogus".into(), "remux_2160p".into()],
            ..Default::default()
        };
        let encoded = serde_html_form::to_string(form.to_query()).unwrap();
        assert!(encoded.contains("media_type=movies"), "{encoded}");
        assert!(
            encoded.contains("categories=2000&categories=5000"),
            "{encoded}"
        );
        assert!(
            encoded.contains("qualities=web_1080p&qualities=remux_2160p"),
            "{encoded}"
        );
    }
}
