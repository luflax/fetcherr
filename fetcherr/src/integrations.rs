//! The live upstream clients, rebuilt whenever settings change.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::redirect;

use crate::error::AppError;
use crate::prowlarr::ProwlarrClient;
use crate::qbittorrent::QbittorrentClient;
use crate::settings::{
    DownloadClientSettings, DownloadClientTestResult, IndexerSettings, IndexerTestResult, Settings,
};

/// qBittorrent tag marking the torrents Fetcherr manages.
pub const DOWNLOAD_TAG: &str = "fetcherr";
const MAX_REDIRECTS: usize = 10;

/// The single HTTP client (connection pool) shared by every upstream integration.
pub fn http_client() -> reqwest::Client {
    install_tls_provider();
    reqwest::Client::builder()
        .user_agent(concat!("fetcherr/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .redirect(redirect::Policy::custom(|attempt| {
            let is_magnet = attempt.url().scheme() == "magnet";
            if is_magnet || attempt.previous().len() >= MAX_REDIRECTS {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .unwrap_or_else(|err| {
            tracing::error!("cannot build HTTP client ({err}); using defaults");
            reqwest::Client::new()
        })
}

/// rustls needs a process-wide crypto provider; `ring` avoids the aws-lc C/CMake toolchain.
fn install_tls_provider() {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[derive(Default)]
pub struct Integrations {
    prowlarr: Option<ProwlarrClient>,
    qbittorrent: Option<QbittorrentClient>,
}

impl Integrations {
    pub fn build(http: &reqwest::Client, settings: &Settings) -> Self {
        let prowlarr = settings
            .indexer
            .is_configured()
            .then(|| ProwlarrClient::new(http.clone(), &settings.indexer))
            .and_then(log_invalid);
        let qbittorrent = settings
            .download_client
            .is_configured()
            .then(|| QbittorrentClient::new(http.clone(), &settings.download_client))
            .and_then(log_invalid);
        Self {
            prowlarr,
            qbittorrent,
        }
    }

    pub fn prowlarr(&self) -> Result<&ProwlarrClient, AppError> {
        self.prowlarr
            .as_ref()
            .ok_or(AppError::NotConfigured("Prowlarr"))
    }

    pub fn qbittorrent(&self) -> Result<&QbittorrentClient, AppError> {
        self.qbittorrent
            .as_ref()
            .ok_or(AppError::NotConfigured("qBittorrent"))
    }
}

fn log_invalid<T>(client: Result<T, crate::upstream::UpstreamError>) -> Option<T> {
    client.inspect_err(|err| tracing::warn!("{err}")).ok()
}

/// Swappable [`Integrations`]; readers clone the `Arc` and never block a settings save.
#[derive(Default)]
pub struct SharedIntegrations(RwLock<Arc<Integrations>>);

impl SharedIntegrations {
    pub fn new(integrations: Integrations) -> Self {
        Self(RwLock::new(Arc::new(integrations)))
    }

    pub fn current(&self) -> Arc<Integrations> {
        match self.0.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn replace(&self, integrations: Integrations) {
        let integrations = Arc::new(integrations);
        match self.0.write() {
            Ok(mut guard) => *guard = integrations,
            Err(poisoned) => *poisoned.into_inner() = integrations,
        }
    }
}

pub async fn test_prowlarr(
    http: &reqwest::Client,
    settings: &IndexerSettings,
) -> Result<IndexerTestResult, AppError> {
    let client = ProwlarrClient::new(http.clone(), settings)?;
    let (version, categories) = tokio::try_join!(client.version(), client.categories())?;
    Ok(IndexerTestResult {
        version,
        categories,
    })
}

pub async fn test_qbittorrent(
    http: &reqwest::Client,
    settings: &DownloadClientSettings,
) -> Result<DownloadClientTestResult, AppError> {
    let client = QbittorrentClient::new(http.clone(), settings)?;
    Ok(DownloadClientTestResult {
        version: client.version().await?,
    })
}
