mod api;
mod cache;
mod catalog;
mod clock;
mod config;
mod db;
mod domain;
mod downloads;
mod error;
mod events;
mod import;
mod integrations;
mod jellyfin;
mod library;
mod metadata;
mod omdb;
mod prowlarr;
mod qbittorrent;
mod quality;
mod release_name;
mod releases;
mod requests;
mod search;
mod server;
mod settings;
mod state;
mod titles;
mod tmdb;
mod torrent;
mod ui;
mod upstream;

use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let config = Config::load();
    init_tracing(&config.log_level);

    let state = AppState::initialize(&config.data_dir)
        .await
        .map_err(|err| std::io::Error::other(err.to_string()))?;
    let listener = server::bind_listener(config.port)?;
    tracing::info!(
        "fetcherr listening on {} (data dir {})",
        listener.local_addr()?,
        config.data_dir.display()
    );

    axum::serve(listener, app(state))
        .with_graceful_shutdown(server::shutdown_signal())
        .await?;
    tracing::info!("fetcherr stopped");
    Ok(())
}

fn init_tracing(log_level: &str) {
    let filter = EnvFilter::try_new(log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn app(state: AppState) -> Router {
    let api = api::router(state);
    let ui = ui::router(ui::ApiClient::new(api.clone()));
    api.merge(ui)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}

#[cfg(test)]
mod tests;
