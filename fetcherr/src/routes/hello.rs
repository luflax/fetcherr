use axum::Json;
use serde_json::{Value, json};

pub async fn hello() -> Json<Value> {
    Json(json!({ "message": "Hello, world!" }))
}

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::AppState;
    use crate::config::Config;
    use crate::routes::router;

    fn app() -> axum::Router {
        router(AppState {
            config: Arc::new(Config {
                log_level: "info".into(),
                port: 0,
            }),
        })
    }

    async fn get_json(uri: &str) -> (StatusCode, Value) {
        let res = app()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn hello_returns_hello_world() {
        let (status, body) = get_json("/api/hello").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({ "message": "Hello, world!" }));
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let (status, body) = get_json("/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({ "status": "ok" }));
    }
}
