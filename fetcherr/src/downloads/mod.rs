//! Downloads managed by Fetcherr: starting them, listing them and streaming their progress.

pub mod feed;
pub mod monitor;

use crate::domain::{Download, DownloadId, MediaType, ReleaseSource, StartDownload};
use crate::error::AppError;
use crate::integrations::{DOWNLOAD_TAG, Integrations};
use crate::qbittorrent::AddTorrent;
use crate::releases::ReleaseCache;
use crate::settings::Settings;
use crate::torrent::TorrentPayload;
use crate::upstream::UpstreamError;

pub use monitor::DownloadMonitor;

/// Sends a searched release to the download client and returns its info-hash as id.
/// The media type decides the qBittorrent category and the default save path.
pub async fn start(
    integrations: &Integrations,
    releases: &ReleaseCache,
    settings: &Settings,
    request: StartDownload,
) -> Result<DownloadId, AppError> {
    let cached = releases
        .get(&request.release_id)
        .ok_or(AppError::ReleaseExpired)?;
    let qbittorrent = integrations.qbittorrent()?;
    let media_type = request.media_type.or(cached.media_type);

    let payload = match cached.source {
        ReleaseSource::Magnet(uri) => TorrentPayload::Magnet(uri),
        ReleaseSource::TorrentUrl(url) => integrations.prowlarr()?.fetch_torrent(&url).await?,
    };
    let id = payload
        .info_hash()
        .map_err(|reason| UpstreamError::new("Prowlarr", format!("unusable torrent: {reason}")))?;

    let save_path = save_path(settings, media_type, request.save_path.as_deref());
    let added = qbittorrent
        .add(&AddTorrent {
            payload: &payload,
            save_path,
            tag: DOWNLOAD_TAG,
            category: media_type.map(MediaType::id),
        })
        .await;

    match added {
        Ok(()) => Ok(id),
        Err(_) if already_in_client(integrations, &id).await => Ok(id),
        Err(err) => Err(err.into()),
    }
}

/// The requested path, else the media type's folder, else the download client's root folder.
fn save_path<'a>(
    settings: &'a Settings,
    media_type: Option<MediaType>,
    requested: Option<&'a str>,
) -> &'a str {
    let media_folder =
        media_type.map(|media_type| settings.media.get(media_type).download_folder.as_str());
    [requested, media_folder]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|path| !path.is_empty())
        .unwrap_or(&settings.download_client.root_folder)
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

pub async fn pause(integrations: &Integrations, id: &DownloadId) -> Result<(), AppError> {
    ensure_managed(integrations, id).await?;
    Ok(integrations
        .qbittorrent()?
        .stop(std::slice::from_ref(id))
        .await?)
}

pub async fn resume(integrations: &Integrations, id: &DownloadId) -> Result<(), AppError> {
    ensure_managed(integrations, id).await?;
    Ok(integrations
        .qbittorrent()?
        .start(std::slice::from_ref(id))
        .await?)
}

/// Removes the download from the client, and its files from disk when `delete_files`.
pub async fn delete(
    integrations: &Integrations,
    id: &DownloadId,
    delete_files: bool,
) -> Result<(), AppError> {
    ensure_managed(integrations, id).await?;
    let ids = std::slice::from_ref(id);
    Ok(integrations
        .qbittorrent()?
        .delete(ids, delete_files)
        .await?)
}

/// qBittorrent answers 200 for any hash, so check it is one of ours before acting on it.
async fn ensure_managed(integrations: &Integrations, id: &DownloadId) -> Result<(), AppError> {
    get(integrations, id).await.map(drop)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_path_prefers_request_then_media_folder_then_root() {
        let mut settings = Settings::default();
        settings.download_client.root_folder = "/downloads".into();
        settings.media.movies.download_folder = "/media/movies".into();

        let movies = Some(MediaType::Movies);
        assert_eq!(save_path(&settings, movies, Some(" /tmp/x ")), "/tmp/x");
        assert_eq!(save_path(&settings, movies, Some("")), "/media/movies");
        assert_eq!(
            save_path(&settings, Some(MediaType::Series), None),
            "/downloads"
        );
        assert_eq!(save_path(&settings, None, None), "/downloads");
    }
}
