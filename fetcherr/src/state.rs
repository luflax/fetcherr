use std::path::Path;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::catalog::Catalog;
use crate::db;
use crate::devices::Devices;
use crate::downloads::DownloadMonitor;
use crate::error::AppError;
use crate::events::EventBus;
use crate::integrations::{self, Integrations, SharedIntegrations};
use crate::library::Library;
use crate::releases::ReleaseCache;
use crate::requests::{RequestDeps, Requests};
use crate::settings::{Settings, SettingsStore};

#[derive(Clone)]
pub struct AppState {
    pub http: reqwest::Client,
    pub settings: Arc<SettingsStore>,
    integrations: Arc<SharedIntegrations>,
    pub releases: Arc<ReleaseCache>,
    pub monitor: DownloadMonitor,
    pub events: EventBus,
    pub library: Library,
    pub requests: Requests,
    pub catalog: Catalog,
    pub devices: Devices,
}

impl AppState {
    pub async fn initialize(data_dir: &Path) -> Result<Self, AppError> {
        let http = integrations::http_client();
        let settings = Arc::new(SettingsStore::open(data_dir).await);
        settings.ensure_api_key().await?;
        let pool: SqlitePool = db::open(data_dir).await?;
        let integrations = Arc::new(SharedIntegrations::new(Integrations::build(
            &http,
            &settings.get(),
        )));
        let monitor = DownloadMonitor::start(integrations.clone(), settings.clone());
        let events = EventBus::default();
        let library = Library::start(integrations.clone(), settings.clone(), events.clone());
        let releases = Arc::new(ReleaseCache::default());
        let requests = Requests::start(RequestDeps {
            pool: pool.clone(),
            integrations: integrations.clone(),
            settings: settings.clone(),
            releases: releases.clone(),
            monitor: monitor.clone(),
            library: library.clone(),
            events: events.clone(),
        })
        .await?;
        let devices = Devices::open(pool.clone()).await?;
        let catalog = Catalog::new(
            integrations.clone(),
            settings.clone(),
            library.clone(),
            requests.clone(),
            pool,
        );
        Ok(Self {
            http,
            settings,
            integrations,
            releases,
            monitor,
            events,
            library,
            requests,
            catalog,
            devices,
        })
    }

    pub fn integrations(&self) -> Arc<Integrations> {
        self.integrations.current()
    }

    /// Persists a settings change and reconnects the integrations with the new values.
    pub async fn update_settings(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<(), AppError>,
    ) -> Result<Arc<Settings>, AppError> {
        let before = self.settings.get();
        let settings = self.settings.update(change).await?;
        self.integrations
            .replace(Integrations::build(&self.http, &settings));
        if before.jellyfin != settings.jellyfin {
            self.library.sync_now();
        }
        Ok(settings)
    }
}
