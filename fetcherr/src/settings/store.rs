use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use tokio::sync::Mutex;

use super::Settings;
use crate::error::AppError;

const FILE_NAME: &str = "settings.json";

/// Settings persisted as JSON in the data dir and cached in memory for cheap reads.
pub struct SettingsStore {
    path: PathBuf,
    current: RwLock<Arc<Settings>>,
    write_lock: Mutex<()>,
}

impl SettingsStore {
    pub async fn open(data_dir: &Path) -> Self {
        let path = data_dir.join(FILE_NAME);
        let settings = read(&path).await;
        Self {
            path,
            current: RwLock::new(Arc::new(settings)),
            write_lock: Mutex::new(()),
        }
    }

    pub fn get(&self) -> Arc<Settings> {
        match self.current.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Applies `change` to a copy of the settings, persists it atomically and publishes it.
    pub async fn update(
        &self,
        change: impl FnOnce(&mut Settings) -> Result<(), AppError>,
    ) -> Result<Arc<Settings>, AppError> {
        let _guard = self.write_lock.lock().await;
        let mut next = (*self.get()).clone();
        change(&mut next)?;
        write(&self.path, &next).await?;

        let next = Arc::new(next);
        match self.current.write() {
            Ok(mut guard) => *guard = next.clone(),
            Err(poisoned) => *poisoned.into_inner() = next.clone(),
        }
        Ok(next)
    }
}

async fn read(path: &Path) -> Settings {
    let raw = match tokio::fs::read_to_string(path).await {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Settings::default(),
        Err(err) => {
            tracing::warn!("cannot read {}: {err}; using defaults", path.display());
            return Settings::default();
        }
    };
    serde_json::from_str(&raw).unwrap_or_else(|err| {
        tracing::warn!("invalid {}: {err}; using defaults", path.display());
        Settings::default()
    })
}

async fn write(path: &Path, settings: &Settings) -> Result<(), AppError> {
    let failed =
        |err: std::io::Error| AppError::Internal(format!("cannot save {}: {err}", path.display()));
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(failed)?;
    }
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|err| AppError::Internal(format!("cannot serialize settings: {err}")))?;
    let temp = path.with_extension("json.tmp");
    tokio::fs::write(&temp, json).await.map_err(failed)?;
    tokio::fs::rename(&temp, path).await.map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn persists_and_reloads() {
        let dir = std::env::temp_dir().join(format!("fetcherr-store-{}", std::process::id()));
        let store = SettingsStore::open(&dir).await;
        assert_eq!(*store.get(), Settings::default());

        store
            .update(|settings| {
                settings.indexer.url = "http://prowlarr:9696".into();
                Ok(())
            })
            .await
            .unwrap();

        let reopened = SettingsStore::open(&dir).await;
        assert_eq!(reopened.get().indexer.url, "http://prowlarr:9696");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn failed_change_keeps_previous_settings() {
        let dir = std::env::temp_dir().join(format!("fetcherr-store-err-{}", std::process::id()));
        let store = SettingsStore::open(&dir).await;
        let result = store
            .update(|_| Err(AppError::Invalid("nope".into())))
            .await;
        assert!(result.is_err());
        assert_eq!(*store.get(), Settings::default());
        let _ = std::fs::remove_dir_all(dir);
    }
}
