//! Jellyfin client: reads the library and asks for rescans. DTOs stay private.

mod dto;

use reqwest::{StatusCode, Url, header};
use serde::de::DeserializeOwned;

use crate::library::{LibraryItem, LibraryItemKind};
use crate::settings::JellyfinSettings;
use crate::upstream::{UpstreamError, ensure_success, parse_base_url};

const SERVICE: &str = "Jellyfin";
const PAGE_SIZE: u32 = 2_000;

pub struct JellyfinClient {
    http: reqwest::Client,
    base: Url,
    authorization: String,
}

impl JellyfinClient {
    pub fn new(http: reqwest::Client, settings: &JellyfinSettings) -> Result<Self, UpstreamError> {
        Ok(Self {
            http,
            base: parse_base_url(SERVICE, &settings.url)?,
            authorization: format!("MediaBrowser Token=\"{}\"", settings.api_key),
        })
    }

    /// Server name and version, for the settings test.
    pub async fn version(&self) -> Result<String, UpstreamError> {
        let info: dto::SystemInfo = self.get_json("System/Info", &[]).await?;
        Ok(format!(
            "{} {}",
            info.server_name.unwrap_or_else(|| "Jellyfin".into()),
            info.version.unwrap_or_default()
        )
        .trim()
        .to_string())
    }

    /// Every movie, series, season and episode on disk (virtual "missing" items excluded).
    pub async fn library_items(&self) -> Result<Vec<LibraryItem>, UpstreamError> {
        let mut items = Vec::new();
        loop {
            let params = [
                ("recursive", "true".to_string()),
                (
                    "includeItemTypes",
                    "Movie,Series,Season,Episode".to_string(),
                ),
                ("fields", "ProviderIds".to_string()),
                ("excludeLocationTypes", "Virtual".to_string()),
                ("enableImages", "false".to_string()),
                ("enableUserData", "false".to_string()),
                ("startIndex", items.len().to_string()),
                ("limit", PAGE_SIZE.to_string()),
            ];
            let page: dto::ItemsResult = self.get_json("Items", &params).await?;
            let received = page.items.len();
            items.extend(page.items.into_iter().filter_map(to_library_item));
            let fetched = items.len().max(received);
            if received == 0 || fetched >= page.total_record_count as usize {
                return Ok(items);
            }
        }
    }

    /// Starts a scan of every library.
    pub async fn refresh_library(&self) -> Result<(), UpstreamError> {
        let url = self.url("Library/Refresh")?;
        let response = self
            .http
            .post(url)
            .header(header::AUTHORIZATION, &self.authorization)
            .send()
            .await
            .map_err(request_failed)?;
        check_auth(response.status())?;
        ensure_success(SERVICE, response).await.map(drop)
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<T, UpstreamError> {
        let response = self
            .http
            .get(self.url(path)?)
            .header(header::AUTHORIZATION, &self.authorization)
            .query(params)
            .send()
            .await
            .map_err(request_failed)?;
        check_auth(response.status())?;
        ensure_success(SERVICE, response)
            .await?
            .json()
            .await
            .map_err(|err| UpstreamError::new(SERVICE, format!("unexpected response: {err}")))
    }

    fn url(&self, path: &str) -> Result<Url, UpstreamError> {
        self.base
            .join(path)
            .map_err(|err| UpstreamError::new(SERVICE, err))
    }
}

fn check_auth(status: StatusCode) -> Result<(), UpstreamError> {
    match status {
        StatusCode::UNAUTHORIZED => Err(UpstreamError::new(SERVICE, "API key rejected")),
        StatusCode::FORBIDDEN => Err(UpstreamError::new(
            SERVICE,
            "API key lacks permission (use an administrator API key)",
        )),
        _ => Ok(()),
    }
}

fn request_failed(err: reqwest::Error) -> UpstreamError {
    UpstreamError::new(SERVICE, format!("request failed: {err}"))
}

fn to_library_item(item: dto::BaseItem) -> Option<LibraryItem> {
    let kind = match item.kind.as_deref()? {
        "Movie" => LibraryItemKind::Movie,
        "Series" => LibraryItemKind::Series,
        "Season" => LibraryItemKind::Season,
        "Episode" => LibraryItemKind::Episode,
        _ => return None,
    };
    let number = |value: Option<i32>| value.and_then(|n| u32::try_from(n).ok());
    let (season_number, episode_number) = match kind {
        LibraryItemKind::Season => (number(item.index_number), None),
        LibraryItemKind::Episode => (number(item.parent_index_number), number(item.index_number)),
        _ => (None, None),
    };
    let tmdb_id = item
        .provider_ids
        .as_ref()
        .and_then(|ids| {
            ids.iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("tmdb"))
                .and_then(|(_, id)| id.as_deref())
        })
        .and_then(|id| id.trim().parse().ok());
    Some(LibraryItem {
        id: item.id,
        kind,
        tmdb_id,
        series_id: item.series_id,
        season_number,
        episode_number,
        episode_number_end: number(item.index_number_end),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer) -> JellyfinClient {
        let settings = JellyfinSettings {
            url: server.uri(),
            api_key: "jf-key".into(),
            ..Default::default()
        };
        JellyfinClient::new(crate::integrations::http_client(), &settings).unwrap()
    }

    #[tokio::test]
    async fn reads_library_items_page_by_page() {
        let server = MockServer::start().await;
        let fixture = std::fs::read_to_string(format!(
            "{}/tests/fixtures/jellyfin-items.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        Mock::given(method("GET"))
            .and(path("/Items"))
            .and(header("authorization", "MediaBrowser Token=\"jf-key\""))
            .and(query_param("excludeLocationTypes", "Virtual"))
            .and(query_param("startIndex", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(fixture, "application/json"))
            .expect(1)
            .mount(&server)
            .await;

        let items = client(&server).library_items().await.unwrap();
        let index = crate::library::LibraryIndex::build(&items);
        assert!(index.movie(603).is_some());
        assert_eq!(index.episodes(1399, 1).len(), 2);
    }

    #[tokio::test]
    async fn explains_a_rejected_key() {
        let server = MockServer::start().await;
        Mock::given(path("/System/Info"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let err = client(&server).version().await.unwrap_err();
        assert!(err.message.contains("rejected"));
    }
}
