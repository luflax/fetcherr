//! Downloads managed by Fetcherr: starting them, listing them and streaming their progress.

pub mod feed;
pub mod monitor;

use crate::domain::{Download, DownloadId, ReleaseSource, StartDownload};
use crate::error::AppError;
use crate::integrations::{DOWNLOAD_TAG, Integrations};
use crate::qbittorrent::AddTorrent;
use crate::releases::ReleaseCache;
use crate::torrent::TorrentPayload;
use crate::upstream::UpstreamError;

pub use monitor::DownloadMonitor;

/// Sends a searched release to the download client and returns its info-hash as id.
pub async fn start(
    integrations: &Integrations,
    releases: &ReleaseCache,
    root_folder: &str,
    request: StartDownload,
) -> Result<DownloadId, AppError> {
    let source = releases
        .get(&request.release_id)
        .ok_or(AppError::ReleaseExpired)?;
    let qbittorrent = integrations.qbittorrent()?;

    let payload = match source {
        ReleaseSource::Magnet(uri) => TorrentPayload::Magnet(uri),
        ReleaseSource::TorrentUrl(url) => integrations.prowlarr()?.fetch_torrent(&url).await?,
    };
    let id = payload
        .info_hash()
        .map_err(|reason| UpstreamError::new("Prowlarr", format!("unusable torrent: {reason}")))?;

    let save_path = request
        .save_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .unwrap_or(root_folder);
    let added = qbittorrent
        .add(&AddTorrent {
            payload: &payload,
            save_path,
            tag: DOWNLOAD_TAG,
        })
        .await;

    match added {
        Ok(()) => Ok(id),
        Err(_) if already_in_client(integrations, &id).await => Ok(id),
        Err(err) => Err(err.into()),
    }
}

async fn already_in_client(integrations: &Integrations, id: &DownloadId) -> bool {
    let Ok(qbittorrent) = integrations.qbittorrent() else {
        return false;
    };
    let ids = std::slice::from_ref(id);
    qbittorrent
        .torrents(None, Some(ids))
        .await
        .is_ok_and(|found| !found.is_empty())
}

/// All Fetcherr downloads, sorted by name for a stable order.
pub async fn list(integrations: &Integrations) -> Result<Vec<Download>, AppError> {
    let mut downloads = integrations
        .qbittorrent()?
        .torrents(Some(DOWNLOAD_TAG), None)
        .await?;
    downloads.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(downloads)
}

pub async fn get(integrations: &Integrations, id: &DownloadId) -> Result<Download, AppError> {
    integrations
        .qbittorrent()?
        .torrents(Some(DOWNLOAD_TAG), Some(std::slice::from_ref(id)))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::NotFound(format!("download {id}")))
}
