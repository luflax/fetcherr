use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Where the Home Assistant Supervisor mounts persistent add-on storage.
const HA_DATA_DIR: &str = "/data";
const DEV_DATA_DIR: &str = "./data";
const DEFAULT_PORT: u16 = 8734;
const DEFAULT_LOG_LEVEL: &str = "info";

/// Options configured by the user in the add-on UI (mirrors `schema` in config.yaml).
#[derive(Debug, Default, Deserialize)]
struct AddonOptions {
    log_level: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub log_level: String,
    pub port: u16,
    pub data_dir: PathBuf,
}

impl Config {
    /// Precedence: HA options file > environment variables > defaults.
    pub fn load() -> Self {
        let data_dir = data_dir();
        let options = read_options(&data_dir.join("options.json")).unwrap_or_default();

        let log_level = options
            .log_level
            .or_else(|| std::env::var("LOG_LEVEL").ok())
            .unwrap_or_else(|| DEFAULT_LOG_LEVEL.to_string());

        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT);

        Self {
            log_level,
            port,
            data_dir,
        }
    }
}

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("DATA_DIR") {
        return PathBuf::from(dir);
    }
    if Path::new(HA_DATA_DIR).is_dir() {
        PathBuf::from(HA_DATA_DIR)
    } else {
        PathBuf::from(DEV_DATA_DIR)
    }
}

fn read_options(path: &Path) -> Option<AddonOptions> {
    let raw = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&raw) {
        Ok(options) => Some(options),
        Err(err) => {
            eprintln!("ignoring invalid {}: {err}", path.display());
            None
        }
    }
}
