use std::path::Path;
use std::sync::Arc;

use crate::downloads::DownloadMonitor;
use crate::error::AppError;
use crate::integrations::{self, Integrations, SharedIntegrations};
use crate::releases::ReleaseCache;
use crate::settings::{Settings, SettingsStore};

#[derive(Clone)]
pub struct AppState {
    pub http: reqwest::Client,
    pub settings: Arc<SettingsStore>,
    integrations: Arc<SharedIntegrations>,
    pub releases: Arc<ReleaseCache>,
    pub monitor: DownloadMonitor,
}

impl AppState {
    pub async fn initialize(data_dir: &Path) -> Self {
        let http = integrations::http_client();
        let settings = Arc::new(SettingsStore::open(data_dir).await);
        let integrations = Arc::new(SharedIntegrations::new(Integrations::build(
            &http,
            &settings.get(),
        )));
        let monitor = DownloadMonitor::start(integrations.clone(), settings.clone());
        Self {
            http,
            settings,
            integrations,
            releases: Arc::new(ReleaseCache::default()),
            monitor,
        }
    }

    pub fn integrations(&self) -> Arc<Integrations> {
        self.integrations.current()
    }

    /// Persists a settings change and reconnects the integrations with the new values.
    pub async fn update_settings(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<(), AppError>,
    ) -> Result<Arc<Settings>, AppError> {
        let settings = self.settings.update(change).await?;
        self.integrations
            .replace(Integrations::build(&self.http, &settings));
        Ok(settings)
    }
}
