use askama::Template;
use axum::extract::State;
use axum::response::Response;
use axum_extra::extract::Query;
use serde::{Deserialize, Serialize};

use super::api_client::ApiClient;
use super::format;
use super::page::{BaseHref, Layout, render};
use super::settings::{CategoryChoices, load_category_choices};
use crate::domain::{Release, SearchQuery, SortBy};
use crate::settings::IndexerSettingsView;

/// Search form exactly as the browser submits it; numbers stay text so blanks are fine.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SearchForm {
    pub query: String,
    pub categories: Vec<u32>,
    pub min_seeders: String,
    pub min_size_gb: String,
    pub max_size_gb: String,
    pub sort: String,
}

impl SearchForm {
    fn to_query(&self) -> SearchQuery {
        SearchQuery {
            query: self.query.trim().to_string(),
            categories: self.categories.clone(),
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

#[derive(Template)]
#[template(path = "search.html")]
struct SearchPage {
    layout: Layout,
    form: SearchForm,
    categories: CategoryChoices,
}

pub async fn page(
    State(api): State<ApiClient>,
    base: BaseHref,
    Query(form): Query<SearchForm>,
) -> Response {
    let defaults = api
        .get::<IndexerSettingsView>("/api/settings/indexer")
        .await
        .map(|settings| settings.categories)
        .unwrap_or_default();
    let selected = if form.categories.is_empty() {
        defaults
    } else {
        form.categories.clone()
    };
    let categories = load_category_choices(&api, "search-categories", &selected, false).await;
    render(SearchPage {
        layout: Layout::new(base, "search"),
        form,
        categories,
    })
}

pub struct ReleaseRow {
    pub release_id: String,
    pub title: String,
    pub info_url: Option<String>,
    pub indexer: String,
    pub age: String,
    pub size: String,
    pub seeders: u32,
    pub leechers: u32,
}

impl From<Release> for ReleaseRow {
    fn from(release: Release) -> Self {
        Self {
            release_id: release.release_id.to_string(),
            title: release.title,
            info_url: release.info_url.filter(|url| is_web_link(url)),
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
        assert_eq!(query.min_seeders, None);
        assert_eq!(query.min_size, Some(1_610_612_736));
        assert_eq!(query.max_size, None);
        assert_eq!(query.sort, SortBy::Age);
    }

    #[test]
    fn serializes_repeated_categories_for_the_api() {
        let query = SearchQuery {
            query: "x".into(),
            categories: vec![2000, 5000],
            ..Default::default()
        };
        let encoded = serde_html_form::to_string(&query).unwrap();
        assert!(
            encoded.contains("categories=2000&categories=5000"),
            "{encoded}"
        );
    }
}
