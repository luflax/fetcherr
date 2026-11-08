//! qBittorrent WebUI client with cookie (SID) sessions. DTOs stay private; callers get domain types.

mod dto;

use reqwest::multipart::{Form, Part};
use reqwest::{RequestBuilder, Response, StatusCode, Url, header};
use tokio::sync::Mutex;

use crate::domain::{Download, DownloadId, DownloadState, MediaType};
use crate::settings::DownloadClientSettings;
use crate::torrent::TorrentPayload;
use crate::upstream::{UpstreamError, ensure_success, parse_base_url};

const SERVICE: &str = "qBittorrent";
const BAD_CREDENTIALS: &str = "invalid username or password";
const BANNED: &str = "IP banned after too many failed logins";
/// qBittorrent reports this ETA for torrents that will never finish.
const ETA_INFINITY: i64 = 8_640_000;

pub struct QbittorrentClient {
    http: reqwest::Client,
    base: Url,
    username: String,
    password: String,
    session: Mutex<Option<Session>>,
}

#[derive(Debug, Clone, PartialEq)]
enum Session {
    Cookie(String),
    /// The WebUI accepted the login without a cookie (e.g. auth bypass for local subnets).
    Anonymous,
}

pub struct AddTorrent<'a> {
    pub payload: &'a TorrentPayload,
    pub save_path: &'a str,
    pub tag: &'a str,
    pub category: Option<&'a str>,
}

impl QbittorrentClient {
    pub fn new(
        http: reqwest::Client,
        settings: &DownloadClientSettings,
    ) -> Result<Self, UpstreamError> {
        Ok(Self {
            http,
            base: parse_base_url(SERVICE, &settings.url)?,
            username: settings.username.clone(),
            password: settings.password.clone(),
            session: Mutex::new(None),
        })
    }

    pub async fn version(&self) -> Result<String, UpstreamError> {
        let response = self
            .send(|url| self.http.get(url), "api/v2/app/version")
            .await?;
        response.text().await.map_err(request_failed)
    }

    pub async fn add(&self, torrent: &AddTorrent<'_>) -> Result<(), UpstreamError> {
        let response = self
            .send(
                |url| self.http.post(url).multipart(add_form(torrent)),
                "api/v2/torrents/add",
            )
            .await?;
        let body = response.text().await.map_err(request_failed)?;
        if body.trim() == "Fails." {
            return Err(UpstreamError::new(
                SERVICE,
                "torrent was rejected (already added or invalid)",
            ));
        }
        Ok(())
    }

    /// Torrents, optionally narrowed to a tag and/or to `ids`.
    pub async fn torrents(
        &self,
        tag: Option<&str>,
        ids: Option<&[DownloadId]>,
    ) -> Result<Vec<Download>, UpstreamError> {
        let mut params = Vec::new();
        if let Some(tag) = tag {
            params.push(("tag", tag.to_string()));
        }
        if let Some(ids) = ids {
            params.push(("hashes", join_hashes(ids)));
        }
        let response = self
            .send(
                |url| self.http.get(url).query(&params),
                "api/v2/torrents/info",
            )
            .await?;
        let torrents: Vec<dto::TorrentInfo> = response
            .json()
            .await
            .map_err(|err| UpstreamError::new(SERVICE, format!("unexpected response: {err}")))?;
        Ok(torrents.into_iter().filter_map(to_download).collect())
    }

    /// Pauses torrents (`stop` is the 5.x name of `pause`).
    pub async fn stop(&self, ids: &[DownloadId]) -> Result<(), UpstreamError> {
        self.post_hashes("api/v2/torrents/stop", ids, &[]).await
    }

    /// Resumes torrents (`start` is the 5.x name of `resume`).
    pub async fn start(&self, ids: &[DownloadId]) -> Result<(), UpstreamError> {
        self.post_hashes("api/v2/torrents/start", ids, &[]).await
    }

    pub async fn delete(
        &self,
        ids: &[DownloadId],
        delete_files: bool,
    ) -> Result<(), UpstreamError> {
        let delete_files = if delete_files { "true" } else { "false" };
        self.post_hashes(
            "api/v2/torrents/delete",
            ids,
            &[("deleteFiles", delete_files)],
        )
        .await
    }

    async fn post_hashes(
        &self,
        path: &str,
        ids: &[DownloadId],
        extra: &[(&str, &str)],
    ) -> Result<(), UpstreamError> {
        let hashes = join_hashes(ids);
        let mut form = vec![("hashes", hashes.as_str())];
        form.extend_from_slice(extra);
        self.send(|url| self.http.post(url).form(&form), path)
            .await
            .map(drop)
    }

    /// Sends an authenticated request, logging in first and once more if the session expired.
    async fn send(
        &self,
        build: impl Fn(Url) -> RequestBuilder,
        path: &str,
    ) -> Result<Response, UpstreamError> {
        let url = self
            .base
            .join(path)
            .map_err(|err| UpstreamError::new(SERVICE, err))?;
        let session = self.current_session().await?;
        let response = self.authorize(build(url.clone()), &session).send().await;
        let response = response.map_err(request_failed)?;
        if !is_unauthenticated(response.status()) {
            return ensure_success(SERVICE, response).await;
        }

        let session = self.renew_session(&session).await?;
        let retry = self.authorize(build(url), &session).send().await;
        ensure_success(SERVICE, retry.map_err(request_failed)?).await
    }

    async fn current_session(&self) -> Result<Session, UpstreamError> {
        let mut session = self.session.lock().await;
        if let Some(current) = session.as_ref() {
            return Ok(current.clone());
        }
        let fresh = self.login().await?;
        *session = Some(fresh.clone());
        Ok(fresh)
    }

    /// Logs in again unless a concurrent request already replaced the stale session.
    async fn renew_session(&self, stale: &Session) -> Result<Session, UpstreamError> {
        let mut session = self.session.lock().await;
        if let Some(current) = session.as_ref().filter(|current| *current != stale) {
            return Ok(current.clone());
        }
        let fresh = self.login().await?;
        *session = Some(fresh.clone());
        Ok(fresh)
    }

    fn authorize(&self, request: RequestBuilder, session: &Session) -> RequestBuilder {
        let request = request.header(header::REFERER, self.base.as_str());
        match session {
            Session::Cookie(cookie) => request.header(header::COOKIE, cookie),
            Session::Anonymous => request,
        }
    }

    async fn login(&self) -> Result<Session, UpstreamError> {
        let url = self
            .base
            .join("api/v2/auth/login")
            .map_err(|err| UpstreamError::new(SERVICE, err))?;
        let form = [("username", &self.username), ("password", &self.password)];
        let response = self
            .http
            .post(url)
            .header(header::REFERER, self.base.as_str())
            .form(&form)
            .send()
            .await
            .map_err(request_failed)?;
        match response.status() {
            StatusCode::FORBIDDEN => return Err(UpstreamError::new(SERVICE, BANNED)),
            StatusCode::UNAUTHORIZED => return Err(UpstreamError::new(SERVICE, BAD_CREDENTIALS)),
            _ => {}
        }
        let response = ensure_success(SERVICE, response).await?;
        let cookie = session_cookie(&response);
        let body = response.text().await.map_err(request_failed)?;
        if body.trim() == "Fails." {
            return Err(UpstreamError::new(SERVICE, BAD_CREDENTIALS));
        }
        Ok(cookie.map_or(Session::Anonymous, Session::Cookie))
    }
}

fn add_form(torrent: &AddTorrent<'_>) -> Form {
    let form = match torrent.payload {
        TorrentPayload::Magnet(uri) => Form::new().text("urls", uri.clone()),
        TorrentPayload::File(bytes) => {
            let part = Part::bytes(bytes.clone())
                .file_name("release.torrent")
                .mime_str("application/x-bittorrent")
                .unwrap_or_else(|_| Part::bytes(bytes.clone()).file_name("release.torrent"));
            Form::new().part("torrents", part)
        }
    };
    // Explicit save path wins even when qBittorrent defaults to Automatic Torrent Management.
    let mut form = form
        .text("tags", torrent.tag.to_string())
        .text("autoTMM", "false");
    if let Some(category) = torrent.category {
        form = form.text("category", category.to_string());
    }
    if !torrent.save_path.is_empty() {
        form = form.text("savepath", torrent.save_path.to_string());
    }
    form
}

fn join_hashes(ids: &[DownloadId]) -> String {
    let hashes: Vec<&str> = ids.iter().map(DownloadId::as_str).collect();
    hashes.join("|")
}

/// Picks the session cookie (`SID`, or a custom `*SID*` name) as a `name=value` pair.
fn session_cookie(response: &Response) -> Option<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|cookie| cookie.split(';').next())
        .find(|pair| {
            pair.split('=')
                .next()
                .is_some_and(|name| name.contains("SID"))
        })
        .map(str::to_string)
}

/// The docs specify 403 for missing sessions; qBittorrent 5.3 answers 401 as well.
fn is_unauthenticated(status: StatusCode) -> bool {
    status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED
}

fn request_failed(err: reqwest::Error) -> UpstreamError {
    UpstreamError::new(SERVICE, format!("request failed: {err}"))
}

fn to_download(torrent: dto::TorrentInfo) -> Option<Download> {
    let id = DownloadId::try_from(torrent.hash).ok()?;
    Some(Download {
        id,
        state: to_state(&torrent.state),
        name: torrent.name,
        progress: torrent.progress.clamp(0.0, 1.0),
        size: non_negative(torrent.size),
        downloaded: non_negative(torrent.completed),
        dl_speed: non_negative(torrent.dlspeed),
        ul_speed: non_negative(torrent.upspeed),
        eta_secs: (0..ETA_INFINITY)
            .contains(&torrent.eta)
            .then_some(torrent.eta as u64),
        seeds: non_negative(torrent.num_seeds) as u32,
        peers: non_negative(torrent.num_leechs) as u32,
        save_path: torrent.save_path,
        media_type: MediaType::from_id(&torrent.category),
    })
}

fn non_negative(value: i64) -> u64 {
    value.max(0) as u64
}

/// Maps the documented `state` values; `stopped*` are the 5.x names of `paused*`.
fn to_state(state: &str) -> DownloadState {
    match state {
        "error" | "missingFiles" => DownloadState::Error,
        "uploading" | "stalledUP" | "forcedUP" => DownloadState::Seeding,
        "pausedUP" | "stoppedUP" | "queuedUP" => DownloadState::Completed,
        "downloading" | "forcedDL" => DownloadState::Downloading,
        "stalledDL" => DownloadState::Stalled,
        "metaDL" | "forcedMetaDL" => DownloadState::FetchingMetadata,
        "pausedDL" | "stoppedDL" => DownloadState::Paused,
        "queuedDL" => DownloadState::Queued,
        "checkingUP" | "checkingDL" | "checkingResumeData" | "allocating" => {
            DownloadState::Checking
        }
        "moving" => DownloadState::Moving,
        _ => DownloadState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string_contains, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const HASH: &str = "8c212779b4abde7c6bc608063a0d008b7e40ce32";

    fn client(server: &MockServer) -> QbittorrentClient {
        let settings = DownloadClientSettings {
            url: server.uri(),
            username: "admin".into(),
            password: "secret".into(),
            ..Default::default()
        };
        QbittorrentClient::new(crate::integrations::http_client(), &settings).unwrap()
    }

    async fn mount_login(server: &MockServer, sid: &str) {
        Mock::given(method("POST"))
            .and(path("/api/v2/auth/login"))
            .and(body_string_contains("username=admin"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Set-Cookie", format!("SID={sid}; HttpOnly; path=/"))
                    .set_body_string("Ok."),
            )
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn lists_tagged_torrents_with_session_cookie() {
        let server = MockServer::start().await;
        mount_login(&server, "abc").await;
        Mock::given(method("GET"))
            .and(path("/api/v2/torrents/info"))
            .and(query_param("tag", "fetcherr"))
            .and(header("cookie", "SID=abc"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "hash": HASH, "name": "debian.iso", "state": "downloading",
                    "progress": 0.5, "size": 100, "completed": 50, "dlspeed": 10,
                    "upspeed": 0, "eta": 8640000, "num_seeds": 3, "num_leechs": 1,
                    "save_path": "/downloads", "category": "series"
                }])),
            )
            .mount(&server)
            .await;

        let downloads = client(&server)
            .torrents(Some("fetcherr"), None)
            .await
            .unwrap();

        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].id.as_str(), HASH);
        assert_eq!(downloads[0].state, DownloadState::Downloading);
        assert_eq!(downloads[0].eta_secs, None);
        assert_eq!(downloads[0].media_type, Some(MediaType::Series));
    }

    #[tokio::test]
    async fn logs_in_again_when_session_expires() {
        let server = MockServer::start().await;
        mount_login(&server, "fresh").await;
        Mock::given(path("/api/v2/app/version"))
            .and(header("cookie", "SID=fresh"))
            .respond_with(ResponseTemplate::new(200).set_body_string("v5.0.0"))
            .mount(&server)
            .await;
        let client = client(&server);
        *client.session.lock().await = Some(Session::Cookie("SID=expired".into()));

        Mock::given(path("/api/v2/app/version"))
            .and(header("cookie", "SID=expired"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        assert_eq!(client.version().await.unwrap(), "v5.0.0");
    }

    #[tokio::test]
    async fn reports_wrong_credentials_answered_with_401() {
        let server = MockServer::start().await;
        Mock::given(path("/api/v2/auth/login"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;
        let err = client(&server).version().await.unwrap_err();
        assert!(err.message.contains("username or password"));
    }

    #[tokio::test]
    async fn reports_wrong_credentials() {
        let server = MockServer::start().await;
        Mock::given(path("/api/v2/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_string("Fails."))
            .mount(&server)
            .await;
        let err = client(&server).version().await.unwrap_err();
        assert!(err.message.contains("username or password"));
    }

    #[tokio::test]
    async fn adds_magnet_with_tag_and_save_path() {
        let server = MockServer::start().await;
        mount_login(&server, "abc").await;
        Mock::given(method("POST"))
            .and(path("/api/v2/torrents/add"))
            .and(body_string_contains("magnet:?xt=urn:btih:"))
            .and(body_string_contains("fetcherr"))
            .and(body_string_contains("/media/movies"))
            .and(body_string_contains("name=\"category\"\r\n\r\nmovies"))
            .and(body_string_contains("name=\"autoTMM\"\r\n\r\nfalse"))
            .respond_with(ResponseTemplate::new(200).set_body_string("Ok."))
            .expect(1)
            .mount(&server)
            .await;

        let payload = TorrentPayload::Magnet(format!("magnet:?xt=urn:btih:{HASH}"));
        client(&server)
            .add(&AddTorrent {
                payload: &payload,
                save_path: "/media/movies",
                tag: "fetcherr",
                category: Some("movies"),
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn stops_starts_and_deletes_by_hash() {
        let server = MockServer::start().await;
        mount_login(&server, "abc").await;
        for action in ["stop", "start"] {
            Mock::given(method("POST"))
                .and(path(format!("/api/v2/torrents/{action}")))
                .and(body_string_contains(format!("hashes={HASH}")))
                .respond_with(ResponseTemplate::new(200))
                .expect(1)
                .mount(&server)
                .await;
        }
        Mock::given(method("POST"))
            .and(path("/api/v2/torrents/delete"))
            .and(body_string_contains(format!("hashes={HASH}")))
            .and(body_string_contains("deleteFiles=true"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = client(&server);
        let ids = [DownloadId::try_from(HASH.to_string()).unwrap()];
        client.stop(&ids).await.unwrap();
        client.start(&ids).await.unwrap();
        client.delete(&ids, true).await.unwrap();
    }
}
