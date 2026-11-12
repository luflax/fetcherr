//! End-to-end router tests: browser/API request → API → services → mocked upstreams.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;
use wiremock::matchers::{body_string_contains, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::state::AppState;

const HASH: &str = "8c212779b4abde7c6bc608063a0d008b7e40ce32";

struct TestApp {
    router: Router,
    data_dir: std::path::PathBuf,
    api_key: String,
    events: crate::events::EventBus,
}

impl Drop for TestApp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

impl TestApp {
    async fn new(name: &str) -> Self {
        let data_dir = std::env::temp_dir().join(format!("fetcherr-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_dir);
        let state = AppState::initialize(&data_dir).await.unwrap();
        let api_key = state.settings.get().api.key.clone();
        let events = state.events.clone();
        Self {
            router: crate::app(state),
            data_dir,
            api_key,
            events,
        }
    }

    /// Sends a request with the API key, like an authenticated client.
    async fn request(&self, mut request: Request<Body>) -> (StatusCode, String) {
        request
            .headers_mut()
            .insert("x-api-key", self.api_key.parse().unwrap());
        self.send(request).await
    }

    /// Sends a request as is.
    async fn send(&self, request: Request<Body>) -> (StatusCode, String) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    async fn get(&self, uri: &str) -> (StatusCode, String) {
        self.request(Request::get(uri).body(Body::empty()).unwrap())
            .await
    }

    async fn json(&self, method: Method, uri: &str, body: Value) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let (status, body) = self.request(request).await;
        (status, serde_json::from_str(&body).unwrap_or(Value::Null))
    }

    async fn form(&self, uri: &str, form: &str) -> (StatusCode, String) {
        let request = Request::post(uri)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(form.to_string()))
            .unwrap();
        self.request(request).await
    }

    async fn configure(&self, prowlarr: &MockServer, qbittorrent: &MockServer) {
        let (status, _) = self
            .json(
                Method::PUT,
                "/api/settings/indexer",
                json!({"url": prowlarr.uri(), "api_key": "key"}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = self
            .json(
                Method::PUT,
                "/api/settings/download-client",
                json!({"url": qbittorrent.uri(), "username": "admin", "password": "pw",
                       "root_folder": "/media/movies", "poll_interval_ms": 1000}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }
}

async fn mock_prowlarr() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "guid": "guid-1", "ageHours": 30.0, "size": 4_294_967_296_i64, "indexerId": 1,
            "indexer": "Public", "title": "Debian 12 Netinst", "publishDate": "2024-01-01T00:00:00Z",
            "magnetUrl": format!("magnet:?xt=urn:btih:{HASH}&dn=debian"),
            "infoUrl": "javascript:alert(1)",
            "categories": [{"id": 2000, "name": "Movies"}], "seeders": 42, "leechers": 3,
            "protocol": "torrent"
        },
            movie_release("dune-bluray", "Dune.2021.1080p.BluRay.x264-GRP", 5, '1'),
            movie_release("dune-web", "Dune.2021.2160p.WEB-DL.x265-GRP", 7, '2'),
            movie_release("dune-cam", "Dune.2021.HDCAM.x264", 9, '3'),
        ])))
        .mount(&server)
        .await;
    Mock::given(path("/api/v1/indexer/categories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 2000, "name": "Movies", "subCategories": []},
            {"id": 5000, "name": "TV", "subCategories": []}
        ])))
        .mount(&server)
        .await;
    server
}

/// A Movies sub-category (2040) release with a magnet link of `hash_digit` repeated.
fn movie_release(guid: &str, title: &str, seeders: u32, hash_digit: char) -> Value {
    json!({
        "guid": guid, "ageHours": 2.0, "size": 1_000_000, "indexerId": 1, "indexer": "Public",
        "title": title, "publishDate": "2024-01-01T00:00:00Z",
        "magnetUrl": format!("magnet:?xt=urn:btih:{}", hash_digit.to_string().repeat(40)),
        "categories": [{"id": 2040, "name": "Movies/HD"}], "seeders": seeders, "leechers": 0,
        "protocol": "torrent"
    })
}

async fn mock_qbittorrent() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/auth/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Set-Cookie", "SID=s1; path=/")
                .set_body_string("Ok."),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v2/torrents/add"))
        .and(body_string_contains("/media/movies"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Ok."))
        .mount(&server)
        .await;
    Mock::given(path("/api/v2/torrents/info"))
        .and(query_param("tag", "fetcherr"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "hash": HASH, "name": "debian-12.iso", "state": "downloading", "progress": 0.25,
            "size": 100, "completed": 25, "dlspeed": 2048, "upspeed": 0, "eta": 90,
            "num_seeds": 5, "num_leechs": 1, "save_path": "/media/movies"
        }])))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn serves_health_and_openapi_docs() {
    let app = TestApp::new("docs").await;
    assert_eq!(app.get("/health").await.0, StatusCode::OK);

    let (status, spec) = app.get("/docs/openapi.json").await;
    assert_eq!(status, StatusCode::OK);
    for documented in [
        "/api/search",
        "/api/downloads",
        "/api/settings/indexer/test",
    ] {
        assert!(spec.contains(documented), "{documented} missing from spec");
    }
    assert_eq!(app.get("/docs/").await.0, StatusCode::OK);
}

#[tokio::test]
async fn settings_hide_secrets_and_persist() {
    let app = TestApp::new("settings").await;
    let (status, saved) = app
        .json(
            Method::PUT,
            "/api/settings/indexer",
            json!({"url": "http://prowlarr:9696", "api_key": "secret", "categories": [2000]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["has_api_key"], true);
    assert!(!saved.to_string().contains("secret"));

    let stored = std::fs::read_to_string(app.data_dir.join("settings.json")).unwrap();
    assert!(stored.contains("http://prowlarr:9696"));

    let (status, error) = app
        .json(
            Method::PUT,
            "/api/settings/indexer",
            json!({"url": "not-a-url"}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error["error"]["code"], "invalid");
}

#[tokio::test]
async fn unconfigured_search_explains_what_to_do() {
    let app = TestApp::new("unconfigured").await;
    let (status, error) = app
        .json(Method::GET, "/api/search?query=x", Value::Null)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "not_configured");
}

#[tokio::test]
async fn api_search_then_download_returns_info_hash() {
    let (prowlarr, qbittorrent) = (mock_prowlarr().await, mock_qbittorrent().await);
    let app = TestApp::new("api-flow").await;
    app.configure(&prowlarr, &qbittorrent).await;

    let (status, releases) = app
        .json(
            Method::GET,
            "/api/search?query=debian&min_seeders=10",
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(releases[0]["seeders"], 42);
    assert!(
        !releases.to_string().contains("magnet:"),
        "sources must stay server-side"
    );

    let release_id = releases[0]["release_id"].clone();
    let (status, started) = app
        .json(
            Method::POST,
            "/api/downloads",
            json!({"release_id": release_id}),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(started["id"], HASH);

    let (status, download) = app
        .json(Method::GET, &format!("/api/downloads/{HASH}"), Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(download["state"], "downloading");
}

#[tokio::test]
async fn unknown_release_is_gone() {
    let app = TestApp::new("gone").await;
    let (status, _) = app
        .json(
            Method::POST,
            "/api/downloads",
            json!({"release_id": "nope"}),
        )
        .await;
    assert_eq!(status, StatusCode::GONE);
}

#[tokio::test]
async fn ui_search_and_download_go_through_the_api() {
    let (prowlarr, qbittorrent) = (mock_prowlarr().await, mock_qbittorrent().await);
    let app = TestApp::new("ui-flow").await;
    app.configure(&prowlarr, &qbittorrent).await;

    let (status, page) = app.get("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        page.contains(r#"value="2000" checked"#),
        "default category preselected"
    );

    let (status, results) = app
        .get("/search/results?query=debian&min_size_gb=&sort=age")
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(results.contains("Debian 12 Netinst"));
    assert!(results.contains("4.0 GB"));
    assert!(
        !results.contains("javascript:"),
        "unsafe indexer links are dropped"
    );

    let release_id = results
        .split(r#"name="release_id" value=""#)
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap()
        .to_string();
    let (status, fragment) = app
        .form("/downloads", &format!("release_id={release_id}"))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(fragment.contains("Added"));
    assert!(fragment.contains("Download started"));

    let (_, downloads) = app.get("/downloads").await;
    assert!(downloads.contains("debian-12.iso"));
    assert!(downloads.contains("25.0%"));
    assert!(downloads.contains("2.0 KB/s"));
}

#[tokio::test]
async fn ui_settings_test_shows_version_and_categories() {
    let prowlarr = mock_prowlarr().await;
    Mock::given(path("/api/v1/system/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"version": "1.30.0"})))
        .mount(&prowlarr)
        .await;
    let app = TestApp::new("ui-test").await;

    let form = format!("url={}&api_key=key", prowlarr.uri());
    let (status, fragment) = app.form("/settings/indexer/test", &form).await;
    assert_eq!(status, StatusCode::OK);
    assert!(fragment.contains("Connected to Prowlarr 1.30.0"));
}

#[tokio::test]
async fn pages_use_the_ingress_prefix_as_base() {
    let app = TestApp::new("ingress").await;
    let request = Request::get("/settings")
        .header("X-Ingress-Path", "/api/hassio_ingress/token123")
        .body(Body::empty())
        .unwrap();
    let (status, page) = app.request(request).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains(r#"<base href="/api/hassio_ingress/token123/">"#));
}

#[tokio::test]
async fn serves_embedded_assets() {
    let app = TestApp::new("assets").await;
    let (status, css) = app.get("/static/app.css").await;
    assert_eq!(status, StatusCode::OK);
    assert!(css.contains("--accent"));
    assert_eq!(app.get("/static/missing.js").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn pause_resume_and_delete_go_through_the_api() {
    let (prowlarr, qbittorrent) = (mock_prowlarr().await, mock_qbittorrent().await);
    for action in ["stop", "start"] {
        Mock::given(method("POST"))
            .and(path(format!("/api/v2/torrents/{action}")))
            .and(body_string_contains(HASH))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&qbittorrent)
            .await;
    }
    Mock::given(method("POST"))
        .and(path("/api/v2/torrents/delete"))
        .and(body_string_contains("deleteFiles=true"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&qbittorrent)
        .await;
    let app = TestApp::new("actions").await;
    app.configure(&prowlarr, &qbittorrent).await;

    let (_, page) = app.get("/downloads").await;
    assert!(page.contains(&format!(r#"hx-post="downloads/{HASH}/pause""#)));

    let (status, toast) = app.form(&format!("/downloads/{HASH}/pause"), "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(toast.contains("Download paused"));
    let (_, toast) = app.form(&format!("/downloads/{HASH}/resume"), "").await;
    assert!(toast.contains("Download resumed"));
    let (_, toast) = app
        .form("/downloads/delete", &format!("id={HASH}&delete_files=on"))
        .await;
    assert!(toast.contains("Download deleted"));

    let request = Request::delete(format!("/api/downloads/{HASH}?delete_files=true"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.request(request).await.0, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn acting_on_a_foreign_torrent_is_not_found() {
    let (prowlarr, qbittorrent) = (mock_prowlarr().await, mock_qbittorrent().await);
    let app = TestApp::new("foreign").await;
    app.configure(&prowlarr, &qbittorrent).await;

    let other = "0".repeat(40);
    Mock::given(path("/api/v2/torrents/info"))
        .and(query_param("hashes", other.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .with_priority(1)
        .mount(&qbittorrent)
        .await;
    let (status, _) = app
        .json(
            Method::POST,
            &format!("/api/downloads/{other}/pause"),
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, toast) = app.form("/downloads/not-a-hash/pause", "").await;
    assert!(toast.contains("toast error"));
}

#[tokio::test]
async fn media_type_search_keeps_allowed_qualities_in_preference_order() {
    let (prowlarr, qbittorrent) = (mock_prowlarr().await, mock_qbittorrent().await);
    let app = TestApp::new("media-search").await;
    app.configure(&prowlarr, &qbittorrent).await;
    let (status, saved) = app
        .json(
            Method::PUT,
            "/api/settings/media-types/movies",
            json!({"categories": [2000], "download_folder": "/media/films",
                   "allowed_qualities": ["bluray_1080p", "web_2160p"]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(
        saved["allowed_qualities"],
        json!(["bluray_1080p", "web_2160p"])
    );

    let (status, releases) = app
        .json(
            Method::GET,
            "/api/search?query=dune&media_type=movies",
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let titles: Vec<&str> = releases
        .as_array()
        .unwrap()
        .iter()
        .map(|release| release["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        [
            "Dune.2021.1080p.BluRay.x264-GRP",
            "Dune.2021.2160p.WEB-DL.x265-GRP"
        ],
        "CAM and unknown qualities are not allowed; Bluray-1080p ranks above the more seeded WEB-2160p"
    );
    assert_eq!(releases[0]["media_type"], "movies");
    let searched = prowlarr.received_requests().await.unwrap();
    assert!(
        searched.iter().any(|request| request
            .url
            .query()
            .unwrap_or("")
            .contains("categories=2000")),
        "searches the media type's categories"
    );

    Mock::given(method("POST"))
        .and(path("/api/v2/torrents/add"))
        .and(body_string_contains("/media/films"))
        .and(body_string_contains("movies"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Ok."))
        .expect(1)
        .mount(&qbittorrent)
        .await;
    let (status, started) = app
        .json(
            Method::POST,
            "/api/downloads",
            json!({"release_id": releases[0]["release_id"]}),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    assert_eq!(started["id"], "1".repeat(40));
}

#[tokio::test]
async fn release_media_type_is_inferred_when_searching_everything() {
    let (prowlarr, qbittorrent) = (mock_prowlarr().await, mock_qbittorrent().await);
    let app = TestApp::new("media-all").await;
    app.configure(&prowlarr, &qbittorrent).await;

    let (_, releases) = app
        .json(Method::GET, "/api/search?query=dune", Value::Null)
        .await;
    let cam = releases
        .as_array()
        .unwrap()
        .iter()
        .find(|release| release["quality"] == "cam")
        .expect("every quality is allowed by default");
    assert_eq!(cam["media_type"], "movies", "2040 belongs to Movies (2000)");
}

#[tokio::test]
async fn ui_media_type_settings_and_search_filters_go_through_the_api() {
    let (prowlarr, qbittorrent) = (mock_prowlarr().await, mock_qbittorrent().await);
    let app = TestApp::new("ui-media").await;
    app.configure(&prowlarr, &qbittorrent).await;

    let (status, page) = app.get("/settings").await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains(r#"hx-post="settings/media/movies""#));
    assert!(page.contains(r#"hx-post="settings/media/software""#));

    let form = "categories=5000&download_folder=%2Fmedia%2Ftv&allowed_qualities=web_720p\
                &allowed_qualities=web_1080p";
    let (status, card) = app.form("/settings/media/series", form).await;
    assert_eq!(status, StatusCode::OK);
    assert!(card.contains("Series settings saved"));
    assert!(card.contains(r#"value="/media/tv""#));
    assert!(
        position(&card, "web_720p") < position(&card, "web_1080p"),
        "the ranking keeps the submitted order"
    );

    let (_, filters) = app.get("/search/filters?media_type=series").await;
    assert!(filters.contains(r#"value="5000" checked"#));
    assert!(filters.contains(r#"value="web_720p" checked"#));
    assert!(!filters.contains(r#"value="cam" checked"#));
    assert!(position(&filters, "web_720p") < position(&filters, "web_1080p"));

    let (_, software) = app.get("/search/filters?media_type=software").await;
    assert!(
        !software.contains(r#"name="qualities""#),
        "Software has no qualities"
    );

    let (status, _) = app
        .json(
            Method::PUT,
            "/api/settings/media-types/software",
            json!({"allowed_qualities": ["web_1080p"]}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

fn position(html: &str, quality: &str) -> usize {
    html.find(&format!(r#"value="{quality}""#))
        .unwrap_or_else(|| panic!("{quality} not rendered"))
}

#[tokio::test]
async fn downloads_page_has_no_live_indicator() {
    let app = TestApp::new("no-live").await;
    let (_, page) = app.get("/downloads").await;
    assert!(!page.contains("live-status"));
}

// ---- Catalog, requests and imports ----

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Polls `check` until it returns `Some`, failing after a few seconds.
async fn eventually<T, F, Fut>(what: &str, mut check: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    for _ in 0..100 {
        if let Some(value) = check().await {
            return value;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for {what}");
}

async fn mock_tmdb() -> MockServer {
    let server = MockServer::start().await;
    for (route, file) in [
        ("/3/movie/11", "tmdb-movie.json"),
        ("/3/tv/1399", "tmdb-series.json"),
        ("/3/tv/1399/season/1", "tmdb-season.json"),
        ("/3/person/31", "tmdb-person.json"),
        ("/3/search/multi", "tmdb-search-multi.json"),
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(fixture(file)))
            .mount(&server)
            .await;
    }
    // Every list (trending, popular, ...) answers with the trending example.
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            r"^/3/(trending|movie|tv)/[a-z_/]+$",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("tmdb-trending-all.json")))
        .mount(&server)
        .await;
    server
}

/// Jellyfin holding the given items, accepting rescans.
async fn mock_jellyfin(items: Value) -> MockServer {
    let server = MockServer::start().await;
    let count = items.as_array().map_or(0, Vec::len);
    Mock::given(method("GET"))
        .and(path("/Items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Items": items, "TotalRecordCount": count, "StartIndex": 0
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/Library/Refresh"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    server
}

impl TestApp {
    async fn configure_catalog(&self, tmdb: &MockServer, jellyfin: &MockServer) {
        for (uri, body) in [
            (
                "/api/settings/tmdb",
                json!({"url": tmdb.uri(), "api_key": "tmdb-key"}),
            ),
            (
                "/api/settings/jellyfin",
                json!({"url": jellyfin.uri(), "api_key": "jf-key"}),
            ),
        ] {
            let (status, saved) = self.json(Method::PUT, uri, body).await;
            assert_eq!(status, StatusCode::OK, "{uri}: {saved}");
        }
    }
}

/// A Prowlarr release with a magnet of `hash_digit` repeated.
fn release(guid: &str, title: &str, category: u32, seeders: u32, hash_digit: char) -> Value {
    json!({
        "guid": guid, "ageHours": 5.0, "size": 2_000_000, "indexerId": 1, "indexer": "Public",
        "title": title, "publishDate": "2024-01-01T00:00:00Z",
        "magnetUrl": format!("magnet:?xt=urn:btih:{}", hash_digit.to_string().repeat(40)),
        "categories": [{"id": category, "name": "x"}], "seeders": seeders, "leechers": 0,
        "protocol": "torrent"
    })
}

async fn mock_prowlarr_with(releases: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(releases))
        .mount(&server)
        .await;
    server
}

/// qBittorrent that accepts torrents and reports `torrents` (and `files` for every hash).
async fn mock_qbittorrent_with(torrents: Value, files: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/auth/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Set-Cookie", "SID=s1; path=/")
                .set_body_string("Ok."),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v2/torrents/add"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Ok."))
        .mount(&server)
        .await;
    Mock::given(path("/api/v2/torrents/info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(torrents))
        .mount(&server)
        .await;
    Mock::given(path("/api/v2/torrents/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(files))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(wiremock::matchers::path_regex(
            r"^/api/v2/torrents/(delete|stop|start)$",
        ))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn api_needs_the_key_but_health_docs_and_pages_do_not() {
    let app = TestApp::new("auth").await;
    let get = |uri: &str| Request::get(uri).body(Body::empty()).unwrap();

    let (status, body) = app.send(get("/api/system/status")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(body.contains("unauthorized"));
    let (status, _) = app.send(get("/api/system/status?api_key=wrong")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app
        .send(get(&format!("/api/system/status?api_key={}", app.api_key)))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.send(get("/api/ws/events")).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "the WebSocket needs the key too"
    );

    assert_eq!(app.send(get("/health")).await.0, StatusCode::OK);
    assert_eq!(app.send(get("/docs/openapi.json")).await.0, StatusCode::OK);
    let (status, page) = app.send(get("/settings")).await;
    assert_eq!(status, StatusCode::OK, "the UI reaches the API in-process");
    assert!(
        page.contains(&app.api_key),
        "the page hands the key to its scripts"
    );

    let (_, status_body) = app
        .json(Method::GET, "/api/system/status", Value::Null)
        .await;
    assert_eq!(status_body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(status_body["services"]["tmdb"], false);
}

#[tokio::test]
async fn regenerating_the_api_key_retires_the_old_one() {
    let app = TestApp::new("api-key").await;
    let (status, fresh) = app
        .json(
            Method::POST,
            "/api/settings/api-key/regenerate",
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let new_key = fresh["key"].as_str().unwrap().to_string();
    assert_ne!(new_key, app.api_key);

    let request = Request::get("/api/system/status")
        .header("x-api-key", &new_key)
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.send(request).await.0, StatusCode::OK);
    let (status, _) = app
        .json(Method::GET, "/api/system/status", Value::Null)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn discover_pages_show_cards_with_availability_and_etags() {
    let tmdb = mock_tmdb().await;
    // The first trending title (Scream VI) is in the library.
    let jellyfin = mock_jellyfin(json!([
        {"Id": "jf-scream", "Type": "Movie", "ProviderIds": {"Tmdb": "934433"}}
    ]))
    .await;
    let app = TestApp::new("discover").await;
    app.configure_catalog(&tmdb, &jellyfin).await;

    let (status, tabs) = app
        .json(Method::GET, "/api/discover/pages", Value::Null)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tabs[0]["id"], "home");

    let card = eventually("the library sync", || async {
        let (_, page) = app
            .json(
                Method::GET,
                "/api/discover/lists/trending?page=1",
                Value::Null,
            )
            .await;
        let card = page["items"][0].clone();
        (card["availability"] == "available").then_some((page, card))
    })
    .await;
    let (page, card) = card;
    assert_eq!(page["page"], 1);
    assert!(page["total_pages"].as_u64().unwrap() > 0);
    assert_eq!(card["key"], "movie:934433");
    assert_eq!(card["jellyfin_item_id"], "jf-scream");
    assert_eq!(
        card["poster"]["small"],
        "https://image.tmdb.org/t/p/w185/wDWwtvkRRlgTiUr6TyLSMX8FCuZ.jpg"
    );

    let request = Request::get("/api/discover/pages/home")
        .body(Body::empty())
        .unwrap();
    let response = {
        let mut request = request;
        request
            .headers_mut()
            .insert("x-api-key", app.api_key.parse().unwrap());
        app.router.clone().oneshot(request).await.unwrap()
    };
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "private, max-age=60");
    let etag = response.headers()["etag"].to_str().unwrap().to_string();
    let again = Request::get("/api/discover/pages/home")
        .header("if-none-match", &etag)
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.request(again).await.0, StatusCode::NOT_MODIFIED);

    let (status, search) = app
        .json(
            Method::GET,
            "/api/titles/search?query=star%20wars",
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(search["items"][0]["result_type"], "title");
    assert_eq!(search["items"][0]["title"], "Star Wars");

    let (status, person) = app.json(Method::GET, "/api/people/31", Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        person["cast"]
            .as_array()
            .unwrap()
            .iter()
            .any(|credit| credit["role"] == "Forrest Gump")
    );
}

#[tokio::test]
async fn a_movie_request_is_grabbed_downloaded_and_imported() {
    let app = TestApp::new("movie-request").await;
    let downloads = app.data_dir.join("downloads");
    let movies = app.data_dir.join("library").join("movies");
    let release_name = "Star.Wars.1977.1080p.BluRay.x264-GRP";
    std::fs::create_dir_all(downloads.join(release_name)).unwrap();
    std::fs::write(
        downloads
            .join(release_name)
            .join(format!("{release_name}.mkv")),
        b"video",
    )
    .unwrap();

    let hash = "c".repeat(40);
    let prowlarr = mock_prowlarr_with(json!([
        release(
            "other",
            "Star.Wars.The.Last.Jedi.2017.2160p.WEB-DL",
            2000,
            99,
            'd'
        ),
        release("right", release_name, 2000, 10, 'c'),
    ]))
    .await;
    let qbittorrent = mock_qbittorrent_with(
        json!([{"hash": hash, "name": release_name, "state": "uploading", "progress": 1.0,
                "size": 5, "completed": 5, "save_path": "/downloads", "category": "movies"}]),
        json!([{"name": format!("{release_name}/{release_name}.mkv"), "size": 5, "progress": 1, "priority": 1}]),
    )
    .await;
    let (tmdb, jellyfin) = (mock_tmdb().await, mock_jellyfin(json!([])).await);
    app.configure(&prowlarr, &qbittorrent).await;
    app.configure_catalog(&tmdb, &jellyfin).await;
    let (status, _) = app
        .json(
            Method::PUT,
            "/api/settings/library",
            json!({
                "movies_folder": movies.to_string_lossy(),
                "path_mappings": [{"remote": "/downloads", "local": downloads.to_string_lossy()}]
            }),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let mut events = app.events.subscribe();

    let (status, created) = app
        .json(
            Method::POST,
            "/api/requests",
            json!({"media_type": "movie", "tmdb_id": 11, "scope": {"type": "movie"}}),
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{created}");
    assert_eq!(created["title"], "Star Wars");
    let id = created["id"].as_i64().unwrap();

    let detail = eventually("the import", || async {
        let (_, detail) = app
            .json(Method::GET, &format!("/api/requests/{id}"), Value::Null)
            .await;
        (detail["state"] == "available").then_some(detail)
    })
    .await;
    assert_eq!(
        detail["torrents"][0]["id"], hash,
        "the matching title, not the higher-seeded other one"
    );
    assert_eq!(detail["torrents"][0]["state"], "imported");
    let imported = movies
        .join("Star Wars (1977) [tmdbid-11]")
        .join("Star Wars (1977).mkv");
    assert_eq!(std::fs::read(&imported).unwrap(), b"video");
    assert!(
        downloads
            .join(release_name)
            .join(format!("{release_name}.mkv"))
            .exists(),
        "hardlinked, still seeding"
    );

    let (status, again) = app
        .json(
            Method::POST,
            "/api/requests",
            json!({"media_type": "movie", "tmdb_id": 11, "scope": {"type": "movie"}}),
        )
        .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the same request is not created twice"
    );
    assert_eq!(again["id"], id);

    use crate::requests::RequestState;
    let mut states = Vec::new();
    let finished = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Ok(event) = events.recv().await {
            if let crate::events::Event::RequestUpdated { request } = event {
                states.push(request.state);
                if request.state == RequestState::Available {
                    return;
                }
            }
        }
    })
    .await;
    assert!(finished.is_ok(), "no available event, got {states:?}");
    assert!(states.contains(&RequestState::Downloading));
    assert!(states.contains(&RequestState::Importing));

    let (_, movie) = app.json(Method::GET, "/api/movies/11", Value::Null).await;
    assert_eq!(movie["card"]["availability"], "available");
    assert_eq!(movie["requests"][0]["state"], "available");

    let (_, list) = app
        .json(
            Method::GET,
            "/api/requests?state=available&page=1",
            Value::Null,
        )
        .await;
    assert_eq!(list["total_results"], 1);

    let (status, _) = app
        .json(
            Method::DELETE,
            &format!("/api/requests/{id}?delete_files=true"),
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        !imported.exists(),
        "imported files are deleted with the request"
    );
}

#[tokio::test]
async fn a_partly_available_season_grabs_only_the_missing_episodes() {
    let app = TestApp::new("season-request").await;
    let prowlarr = mock_prowlarr_with(json!([
        release(
            "pack",
            "Game.of.Thrones.S01.1080p.BluRay.x264",
            5000,
            50,
            'e'
        ),
        release(
            "e03",
            "Game.of.Thrones.S01E03.1080p.WEB.h264",
            5000,
            20,
            'f'
        ),
        release(
            "other",
            "House.of.the.Dragon.S01E03.1080p.WEB",
            5000,
            90,
            '9'
        ),
    ]))
    .await;
    let downloading = |digit: char| {
        json!({
            "hash": digit.to_string().repeat(40), "name": "x", "state": "downloading", "progress": 0.1,
            "size": 100, "completed": 10, "save_path": "/downloads", "category": "series"
        })
    };
    let qbittorrent =
        mock_qbittorrent_with(json!([downloading('e'), downloading('f')]), json!([])).await;
    let tmdb = mock_tmdb().await;
    let jellyfin = mock_jellyfin(fixture("jellyfin-items.json")["Items"].clone()).await;
    app.configure(&prowlarr, &qbittorrent).await;
    app.configure_catalog(&tmdb, &jellyfin).await;

    let season = eventually("the library sync", || async {
        let (_, season) = app
            .json(Method::GET, "/api/series/1399/seasons/1", Value::Null)
            .await;
        (season["availability"] == "partially_available").then_some(season)
    })
    .await;
    assert_eq!(season["episodes"][0]["availability"], "available");
    assert_eq!(season["episodes"][2]["availability"], "unavailable");

    let (status, created) = app
        .json(Method::POST, "/api/requests", json!({"media_type": "series", "tmdb_id": 1399, "scope": {"type": "season", "season": 1}}))
        .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{created}");
    let id = created["id"].as_i64().unwrap();

    let detail = eventually("the grabs", || async {
        let (_, detail) = app
            .json(Method::GET, &format!("/api/requests/{id}"), Value::Null)
            .await;
        (detail["state"] == "downloading").then_some(detail)
    })
    .await;
    let torrents = detail["torrents"].as_array().unwrap();
    assert_eq!(torrents.len(), 2, "{detail}");
    let coverage_of = |hash_digit: char| {
        let hash = hash_digit.to_string().repeat(40);
        torrents
            .iter()
            .find(|torrent| torrent["id"] == hash.as_str())
            .unwrap()["coverage"]
            .clone()
    };
    assert_eq!(
        coverage_of('f'),
        json!({"type": "episodes", "season": 1, "episodes": [3]})
    );
    assert_eq!(
        coverage_of('e'),
        json!({"type": "episodes", "season": 1, "episodes": [4, 5, 6, 7, 8, 9, 10]}),
        "the pack only fills the episodes nobody released alone"
    );

    let (_, season) = app
        .json(Method::GET, "/api/series/1399/seasons/1", Value::Null)
        .await;
    assert_eq!(season["availability"], "downloading");
    assert_eq!(season["episodes"][4]["request_state"], "downloading");

    let (status, releases) = app
        .json(
            Method::GET,
            "/api/series/1399/releases?season=1&episode=3",
            Value::Null,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let titles: Vec<&str> = releases
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"Game.of.Thrones.S01E03.1080p.WEB.h264"));
    assert!(
        !titles.iter().any(|title| title.starts_with("House")),
        "other titles are dropped"
    );
}

#[tokio::test]
async fn layout_settings_are_validated_and_ordered() {
    let app = TestApp::new("layout").await;
    let (status, layout) = app
        .json(
            Method::PUT,
            "/api/settings/layout",
            json!({"pages": [
                {"name": "Anime", "lists": [
                    {"title": "Popular anime", "source": {"kind": "discover", "media": "series"},
                     "filters": {"genres": [16], "original_language": "ja"}},
                    {"title": "Trending", "source": {"kind": "trending"}}
                ]}
            ]}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{layout}");
    assert_eq!(layout["pages"][0]["id"], "anime");
    assert_eq!(layout["pages"][0]["lists"][0]["id"], "popular-anime");

    let (_, tabs) = app
        .json(Method::GET, "/api/discover/pages", Value::Null)
        .await;
    assert_eq!(tabs, json!([{"id": "anime", "name": "Anime"}]));

    let (status, _) = app
        .json(
            Method::PUT,
            "/api/settings/layout",
            json!({"pages": [{"name": " ", "lists": []}]}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (_, page) = app.get("/settings").await;
    assert!(
        page.contains("Popular anime"),
        "the editor shows the saved lists"
    );
}
