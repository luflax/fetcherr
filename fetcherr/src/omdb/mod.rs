//! OMDb client: IMDb, Rotten Tomatoes and Metacritic scores by IMDb id. DTOs stay private.

mod dto;

use reqwest::{StatusCode, Url};

use crate::metadata::{ExternalRatings, ImdbRating};
use crate::settings::OmdbSettings;
use crate::upstream::{UpstreamError, ensure_success, parse_base_url};

const SERVICE: &str = "OMDb";
const DEFAULT_URL: &str = "https://www.omdbapi.com";
const ROTTEN_TOMATOES: &str = "Rotten Tomatoes";

pub struct OmdbClient {
    http: reqwest::Client,
    base: Url,
    api_key: String,
}

impl OmdbClient {
    pub fn new(http: reqwest::Client, settings: &OmdbSettings) -> Result<Self, UpstreamError> {
        let url = if settings.url.trim().is_empty() {
            DEFAULT_URL
        } else {
            &settings.url
        };
        Ok(Self {
            http,
            base: parse_base_url(SERVICE, url)?,
            api_key: settings.api_key.clone(),
        })
    }

    pub async fn ratings(&self, imdb_id: &str) -> Result<ExternalRatings, UpstreamError> {
        let response = self
            .http
            .get(self.base.clone())
            .query(&[("i", imdb_id), ("apikey", &self.api_key)])
            .send()
            .await
            // The URL holds the API key.
            .map_err(|err| {
                UpstreamError::new(SERVICE, format!("request failed: {}", err.without_url()))
            })?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(UpstreamError::new(SERVICE, "API key rejected"));
        }
        let title: dto::TitleResponse = ensure_success(SERVICE, response)
            .await?
            .json()
            .await
            .map_err(|err| {
                UpstreamError::new(
                    SERVICE,
                    format!("unexpected response: {}", err.without_url()),
                )
            })?;
        if title.response.as_deref() == Some("False") {
            let reason = title.error.unwrap_or_else(|| "unknown error".into());
            return Err(
                if reason.to_ascii_lowercase().contains("not found")
                    || reason.contains("Incorrect IMDb ID")
                {
                    UpstreamError::not_found(SERVICE, reason)
                } else {
                    UpstreamError::new(SERVICE, reason)
                },
            );
        }
        Ok(to_ratings(title))
    }
}

fn to_ratings(title: dto::TitleResponse) -> ExternalRatings {
    let imdb = parse_number::<f64>(title.imdb_rating.as_deref()).map(|score| ImdbRating {
        score,
        votes: parse_number(
            title
                .imdb_votes
                .as_deref()
                .map(|votes| votes.replace(',', ""))
                .as_deref(),
        ),
    });
    let rotten_tomatoes = title
        .ratings
        .iter()
        .find(|rating| rating.source == ROTTEN_TOMATOES)
        .and_then(|rating| parse_number(Some(rating.value.trim_end_matches('%'))));
    ExternalRatings {
        imdb,
        rotten_tomatoes,
        metacritic: parse_number(title.metascore.as_deref()),
    }
}

/// Parses an OMDb value, where `"N/A"` means missing.
fn parse_number<T: std::str::FromStr>(value: Option<&str>) -> Option<T> {
    value.map(str::trim).filter(|v| *v != "N/A")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer) -> OmdbClient {
        let settings = OmdbSettings {
            url: server.uri(),
            api_key: "omdb-key".into(),
        };
        OmdbClient::new(crate::integrations::http_client(), &settings).unwrap()
    }

    #[tokio::test]
    async fn reads_imdb_rotten_tomatoes_and_metacritic() {
        let server = MockServer::start().await;
        let fixture = std::fs::read_to_string(format!(
            "{}/tests/fixtures/omdb-title.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        Mock::given(method("GET"))
            .and(query_param("i", "tt0133093"))
            .and(query_param("apikey", "omdb-key"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(fixture, "application/json"))
            .mount(&server)
            .await;
        let ratings = client(&server).ratings("tt0133093").await.unwrap();
        assert_eq!(
            ratings.imdb,
            Some(ImdbRating {
                score: 8.7,
                votes: Some(2_212_391)
            })
        );
        assert_eq!(ratings.rotten_tomatoes, Some(83));
        assert_eq!(ratings.metacritic, Some(73));
    }

    #[tokio::test]
    async fn reports_errors_sent_with_status_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "Response": "False", "Error": "Incorrect IMDb ID."
            })))
            .mount(&server)
            .await;
        let err = client(&server).ratings("tt0").await.unwrap_err();
        assert!(err.not_found);
    }

    #[test]
    fn treats_n_a_as_missing() {
        assert_eq!(parse_number::<f64>(Some("N/A")), None);
        assert_eq!(parse_number::<u32>(Some(" 73 ")), Some(73));
    }
}
