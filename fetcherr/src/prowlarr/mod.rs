//! Prowlarr indexer client. DTOs stay private; callers get domain types.

mod dto;

use reqwest::{StatusCode, Url, header};

use crate::domain::{Category, FoundRelease, Release, ReleaseId, ReleaseSource};
use crate::settings::IndexerSettings;
use crate::torrent::TorrentPayload;
use crate::upstream::{UpstreamError, ensure_success, parse_base_url};

const SERVICE: &str = "Prowlarr";
const API_KEY_HEADER: &str = "X-Api-Key";

pub struct ProwlarrClient {
    http: reqwest::Client,
    base: Url,
    api_key: String,
}

#[derive(Debug, Default)]
pub struct SearchRequest<'a> {
    pub query: &'a str,
    pub categories: &'a [u32],
    pub indexer_ids: &'a [u32],
    pub limit: Option<u32>,
}

impl ProwlarrClient {
    pub fn new(http: reqwest::Client, settings: &IndexerSettings) -> Result<Self, UpstreamError> {
        Ok(Self {
            http,
            base: parse_base_url(SERVICE, &settings.url)?,
            api_key: settings.api_key.clone(),
        })
    }

    pub async fn version(&self) -> Result<String, UpstreamError> {
        let status: dto::SystemResource = self.get_json("api/v1/system/status", &[]).await?;
        Ok(status.version.unwrap_or_else(|| "unknown".into()))
    }

    pub async fn categories(&self) -> Result<Vec<Category>, UpstreamError> {
        let categories: Vec<dto::IndexerCategory> =
            self.get_json("api/v1/indexer/categories", &[]).await?;
        Ok(categories.into_iter().map(to_category).collect())
    }

    /// Searches all indexers; only torrent releases are returned.
    pub async fn search(
        &self,
        request: &SearchRequest<'_>,
    ) -> Result<Vec<FoundRelease>, UpstreamError> {
        let mut params = vec![
            ("query", request.query.to_string()),
            ("type", "search".into()),
        ];
        params.extend(
            request
                .categories
                .iter()
                .map(|c| ("categories", c.to_string())),
        );
        params.extend(
            request
                .indexer_ids
                .iter()
                .map(|i| ("indexerIds", i.to_string())),
        );
        if let Some(limit) = request.limit {
            params.push(("limit", limit.to_string()));
        }
        let releases: Vec<dto::ReleaseResource> = self.get_json("api/v1/search", &params).await?;
        Ok(releases.into_iter().filter_map(to_found_release).collect())
    }

    /// Downloads a release's `.torrent`, or follows Prowlarr's redirect to a magnet link.
    pub async fn fetch_torrent(&self, download_url: &str) -> Result<TorrentPayload, UpstreamError> {
        let url = Url::parse(download_url)
            .map_err(|err| UpstreamError::new(SERVICE, format!("invalid download URL: {err}")))?;
        let mut request = self.http.get(url.clone());
        if url.origin() == self.base.origin() {
            request = request.header(API_KEY_HEADER, &self.api_key);
        }
        let response = request.send().await.map_err(request_failed)?;

        if let Some(magnet) = magnet_redirect(&response) {
            return Ok(TorrentPayload::Magnet(magnet));
        }
        let response = ensure_success(SERVICE, response).await?;
        let bytes = response.bytes().await.map_err(request_failed)?;
        Ok(TorrentPayload::File(bytes.to_vec()))
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<T, UpstreamError> {
        let url = self
            .base
            .join(path)
            .map_err(|err| UpstreamError::new(SERVICE, err))?;
        let response = self
            .http
            .get(url)
            .header(API_KEY_HEADER, &self.api_key)
            .query(params)
            .send()
            .await
            .map_err(request_failed)?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(UpstreamError::new(SERVICE, "API key rejected"));
        }
        ensure_success(SERVICE, response)
            .await?
            .json()
            .await
            .map_err(|err| UpstreamError::new(SERVICE, format!("unexpected response: {err}")))
    }
}

fn request_failed(err: reqwest::Error) -> UpstreamError {
    UpstreamError::new(SERVICE, format!("request failed: {err}"))
}

fn magnet_redirect(response: &reqwest::Response) -> Option<String> {
    if !response.status().is_redirection() {
        return None;
    }
    let location = response.headers().get(header::LOCATION)?.to_str().ok()?;
    location
        .starts_with("magnet:")
        .then(|| location.to_string())
}

/// Prowlarr often hands out its own proxy URL in `magnetUrl` (redirecting to the magnet),
/// so only a literal `magnet:` link is used as-is; any other link is fetched later.
fn release_source(
    magnet_url: Option<String>,
    download_url: Option<String>,
) -> Option<ReleaseSource> {
    match (magnet_url, download_url) {
        (Some(magnet), _) if magnet.starts_with("magnet:") => Some(ReleaseSource::Magnet(magnet)),
        (_, Some(url)) | (Some(url), None) => Some(ReleaseSource::TorrentUrl(url)),
        (None, None) => None,
    }
}

fn to_category(category: dto::IndexerCategory) -> Category {
    Category {
        id: category.id.max(0) as u32,
        name: category.name.unwrap_or_default(),
        sub_categories: category
            .sub_categories
            .unwrap_or_default()
            .into_iter()
            .map(to_category)
            .collect(),
    }
}

fn to_found_release(resource: dto::ReleaseResource) -> Option<FoundRelease> {
    if resource.protocol != dto::DownloadProtocol::Torrent {
        return None;
    }
    let source = release_source(resource.magnet_url, resource.download_url)?;
    let title = resource.title.unwrap_or_default();
    let guid = resource.guid.unwrap_or_else(|| title.clone());
    let release = Release {
        release_id: ReleaseId::from_guid(resource.indexer_id, &guid),
        title,
        indexer: resource.indexer.unwrap_or_default(),
        indexer_id: resource.indexer_id,
        size: resource.size.max(0) as u64,
        seeders: resource.seeders.unwrap_or(0).max(0) as u32,
        leechers: resource.leechers.unwrap_or(0).max(0) as u32,
        published_at: resource.publish_date.unwrap_or_default(),
        age_hours: resource.age_hours,
        categories: resource
            .categories
            .unwrap_or_default()
            .iter()
            .map(|c| c.id.max(0) as u32)
            .collect(),
        info_url: resource.info_url,
    };
    Some(FoundRelease { release, source })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer) -> ProwlarrClient {
        let settings = IndexerSettings {
            url: server.uri(),
            api_key: "key".into(),
            categories: vec![],
        };
        ProwlarrClient::new(crate::integrations::http_client(), &settings).unwrap()
    }

    #[tokio::test]
    async fn search_maps_torrent_releases_only() {
        let server = MockServer::start().await;
        let body = serde_json::json!([
            {
                "guid": "g1", "ageHours": 5.0, "size": 1000, "indexerId": 3, "indexer": "Idx",
                "title": "Movie 2024 1080p", "publishDate": "2024-05-01T00:00:00Z",
                "downloadUrl": "http://prowlarr/3/download?apikey=key",
                "categories": [{"id": 2000, "name": "Movies"}],
                "magnetUrl": null, "seeders": 12, "leechers": 1, "protocol": "torrent"
            },
            {
                "guid": "g2", "indexerId": 4, "title": "Usenet thing",
                "downloadUrl": "http://prowlarr/4/download", "protocol": "usenet"
            }
        ]);
        Mock::given(method("GET"))
            .and(path("/api/v1/search"))
            .and(header(API_KEY_HEADER, "key"))
            .and(query_param("query", "movie"))
            .and(query_param("categories", "2000"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let found = client(&server)
            .search(&SearchRequest {
                query: "movie",
                categories: &[2000],
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].release.seeders, 12);
        assert_eq!(found[0].release.categories, vec![2000]);
        assert_eq!(
            found[0].source,
            ReleaseSource::TorrentUrl("http://prowlarr/3/download?apikey=key".into())
        );
    }

    #[test]
    fn maps_real_search_payload_with_proxied_magnet_links() {
        let raw = include_str!("../../tests/fixtures/prowlarr-search.json");
        let releases: Vec<dto::ReleaseResource> = serde_json::from_str(raw).unwrap();
        let found: Vec<FoundRelease> = releases.into_iter().filter_map(to_found_release).collect();

        assert_eq!(found.len(), 3);
        assert!(
            matches!(&found[0].source, ReleaseSource::TorrentUrl(url) if url.contains("/download?"))
        );
        assert_eq!(found[0].release.indexer, "LinuxTracker");
    }

    #[tokio::test]
    async fn rejected_api_key_is_reported() {
        let server = MockServer::start().await;
        Mock::given(path("/api/v1/system/status"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let err = client(&server).version().await.unwrap_err();
        assert!(err.message.contains("API key"));
    }

    #[tokio::test]
    async fn follows_magnet_redirects_without_fetching_them() {
        let server = MockServer::start().await;
        let magnet = "magnet:?xt=urn:btih:8c212779b4abde7c6bc608063a0d008b7e40ce32";
        Mock::given(path("/1/download"))
            .respond_with(ResponseTemplate::new(301).insert_header("Location", magnet))
            .mount(&server)
            .await;
        let payload = client(&server)
            .fetch_torrent(&format!("{}/1/download", server.uri()))
            .await
            .unwrap();
        assert_eq!(payload, TorrentPayload::Magnet(magnet.into()));
    }
}
