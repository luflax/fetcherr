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
        let state = AppState::initialize(&data_dir).await;
        Self {
            router: crate::app(state),
            data_dir,
        }
    }

    async fn request(&self, request: Request<Body>) -> (StatusCode, String) {
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
                json!({"url": prowlarr.uri(), "api_key": "key", "categories": [2000]}),
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
        }])))
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

    let form = format!("url={}&api_key=key&categories=5000", prowlarr.uri());
    let (status, fragment) = app.form("/settings/indexer/test", &form).await;
    assert_eq!(status, StatusCode::OK);
    assert!(fragment.contains("Connected to Prowlarr 1.30.0"));
    assert!(fragment.contains(r#"value="5000" checked"#));
    assert!(fragment.contains("hx-swap-oob"));
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
