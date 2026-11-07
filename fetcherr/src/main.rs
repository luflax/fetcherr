mod config;
mod routes;

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

/// State shared by all handlers. Future upstream clients (e.g. a `reqwest::Client`) go here.
#[derive(Clone)]
pub struct AppState {
    #[allow(dead_code)]
    pub config: Arc<Config>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let state = AppState {
        config: Arc::new(config),
    };

    let listener = TcpListener::bind(addr).await?;
    tracing::info!("fetcherr listening on {addr}");

    axum::serve(listener, routes::router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("fetcherr stopped");
    Ok(())
}

/// Resolves on SIGINT (Ctrl+C) or SIGTERM (sent by Docker / the HA Supervisor on stop).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
