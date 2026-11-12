//! The request lifecycle in the background:
//!
//! 1. **Queue** (`run_queue`): works out what is still missing and grabs the best releases.
//! 2. **Tracker** (`track_downloads`): follows the grabbed torrents through the shared download
//!    monitor (subscribed only while something downloads) and starts imports.
//! 3. **Import**: places the files in the library, asks Jellyfin to rescan, then queues the
//!    request again so it becomes available or searches for what is still missing.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use super::model::{RequestId, RequestState, Scope, TorrentState};
use super::store::{self, StoredFile, StoredRequest, StoredTorrent};
use super::{Inner, plan, transfer_totals};
use crate::clock;
use crate::domain::{Download, DownloadState};
use crate::downloads::{self, monitor::Snapshot};
use crate::error::AppError;
use crate::events::Event;
use crate::import::{self, ImportTitle};
use crate::quality;
use crate::release_name::{self, Coverage};
use crate::releases::CachedRelease;
use crate::search::{self, TitleQuery, TitleRelease};
use crate::settings::ImportMode;
use crate::titles::TitleKind;

/// Not-found requests are searched again this often.
const RETRY_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
/// Fallback wake-up of the tracker when nothing notifies it.
const TRACK_INTERVAL: Duration = Duration::from_secs(60);
/// Per-episode searches when a season search did not find every episode.
const MAX_EPISODE_SEARCHES: usize = 10;

pub(super) async fn run_queue(inner: Arc<Inner>, mut receiver: mpsc::UnboundedReceiver<RequestId>) {
    while let Some(id) = receiver.recv().await {
        inner.dequeued(id);
        if let Err(err) = process(&inner, id).await {
            tracing::warn!("request {id}: {err}");
            let _ = inner
                .set_state(id, RequestState::NotFound, Some(&err.to_string()))
                .await;
        }
    }
}

pub(super) async fn retry_periodically(inner: Arc<Inner>) {
    let mut ticker =
        tokio::time::interval_at(tokio::time::Instant::now() + RETRY_INTERVAL, RETRY_INTERVAL);
    loop {
        ticker.tick().await;
        match store::in_states(&inner.deps.pool, &[RequestState::NotFound]).await {
            Ok(requests) => requests
                .iter()
                .for_each(|request| inner.enqueue(request.id)),
            Err(err) => tracing::warn!("cannot list requests to retry: {err}"),
        }
    }
}

/// Brings one request forward: done, downloading, or searched and grabbed.
async fn process(inner: &Inner, id: RequestId) -> Result<(), AppError> {
    let Some(request) = store::get(&inner.deps.pool, id).await? else {
        return Ok(());
    };
    if matches!(
        request.state,
        RequestState::Paused | RequestState::Failed | RequestState::Available
    ) {
        return Ok(());
    }
    match request.key.kind {
        TitleKind::Movie => process_movie(inner, &request).await,
        TitleKind::Series => process_series(inner, &request).await,
    }
}

fn is_transferring(torrent: &StoredTorrent) -> bool {
    matches!(
        torrent.state,
        TorrentState::Downloading | TorrentState::Importing
    )
}

async fn process_movie(inner: &Inner, request: &StoredRequest) -> Result<(), AppError> {
    let pool = &inner.deps.pool;
    let torrents = store::torrents_of(pool, request.id).await?;
    if torrents.iter().any(is_transferring) {
        return inner
            .set_state(request.id, RequestState::Downloading, None)
            .await;
    }
    let in_library = inner
        .deps
        .library
        .index()
        .movie(request.key.tmdb_id)
        .is_some();
    if in_library || !store::files_of(pool, request.id).await?.is_empty() {
        return inner
            .set_state(request.id, RequestState::Available, None)
            .await;
    }

    if let Some(cached) = take_manual_release(inner, request).await? {
        grab_cached(inner, request, &cached, None, Coverage::Movie).await?;
        return inner
            .set_state(request.id, RequestState::Downloading, None)
            .await;
    }
    let candidates = search_releases(inner, request, None, None).await?;
    match plan::best_movie(&candidates) {
        Some(release) => {
            grab(inner, request, release, Coverage::Movie).await?;
            inner
                .set_state(request.id, RequestState::Downloading, None)
                .await
        }
        None => {
            inner
                .set_state(
                    request.id,
                    RequestState::NotFound,
                    Some("no matching release found"),
                )
                .await
        }
    }
}

async fn process_series(inner: &Inner, request: &StoredRequest) -> Result<(), AppError> {
    let pool = &inner.deps.pool;
    let tmdb_id = request.key.tmdb_id;
    let season = request
        .scope
        .season()
        .ok_or_else(|| AppError::Internal("series request without a season".into()))?;

    let wanted: BTreeSet<u32> = match &request.scope {
        Scope::Episodes { episodes, .. } => episodes.iter().copied().collect(),
        _ => {
            let integrations = inner.deps.integrations.current();
            integrations
                .tmdb()?
                .season(tmdb_id, season)
                .await?
                .aired_episodes(&clock::today())
        }
    };
    if wanted.is_empty() {
        let reason = format!("season {season} has no aired episodes yet");
        return inner
            .set_state(request.id, RequestState::NotFound, Some(&reason))
            .await;
    }

    let own_torrents = store::torrents_of(pool, request.id).await?;
    let own_active = own_torrents.iter().any(is_transferring);
    let covered = plan::covered_episodes(
        season,
        &inner.deps.library.index().episodes(tmdb_id, season),
        &store::files_for_title(pool, request.key).await?,
        &store::torrents_for_title(pool, request.key).await?,
    );
    let missing: BTreeSet<u32> = wanted.difference(&covered).copied().collect();
    if missing.is_empty() {
        let state = if own_active {
            RequestState::Downloading
        } else {
            RequestState::Available
        };
        return inner.set_state(request.id, state, None).await;
    }

    if let Some(cached) = take_manual_release(inner, request).await? {
        let serves: Vec<u32> = match release_name::parse(&cached.title).coverage {
            Coverage::Episodes {
                season: s,
                episodes,
            } if s == season => episodes
                .into_iter()
                .filter(|e| missing.contains(e))
                .collect(),
            coverage if coverage.covers_season(season) => missing.iter().copied().collect(),
            _ => Vec::new(),
        };
        if !serves.is_empty() {
            let assigned = Coverage::Episodes {
                season,
                episodes: serves,
            };
            grab_cached(inner, request, &cached, None, assigned).await?;
            return inner
                .set_state(request.id, RequestState::Downloading, None)
                .await;
        }
    }

    let grabs = plan_season(inner, request, season, &wanted, &missing).await?;
    let mut still_missing = missing.clone();
    for (release, episodes) in &grabs {
        let assigned = Coverage::Episodes {
            season,
            episodes: episodes.clone(),
        };
        grab(inner, request, release, assigned).await?;
        for episode in episodes {
            still_missing.remove(episode);
        }
    }

    let note = (!still_missing.is_empty()).then(|| {
        let episodes: Vec<String> = still_missing
            .iter()
            .map(|e| format!("S{season:02}E{e:02}"))
            .collect();
        format!("not found yet: {}", episodes.join(", "))
    });
    if grabs.is_empty() && !own_active {
        let reason = note.unwrap_or_else(|| "no matching release found".into());
        return inner
            .set_state(request.id, RequestState::NotFound, Some(&reason))
            .await;
    }
    inner
        .set_state(request.id, RequestState::Downloading, note.as_deref())
        .await
}

/// Releases to grab for the missing episodes: a season pack when the whole season is missing,
/// else single episodes, with a pack as the fallback for episodes nobody released alone.
async fn plan_season(
    inner: &Inner,
    request: &StoredRequest,
    season: u32,
    wanted: &BTreeSet<u32>,
    missing: &BTreeSet<u32>,
) -> Result<Vec<(TitleRelease, Vec<u32>)>, AppError> {
    let candidates = search_releases(inner, request, Some(season), None).await?;
    if missing == wanted
        && let Some(pack) = plan::best_pack(&candidates, season)
    {
        return Ok(vec![(pack.clone(), missing.iter().copied().collect())]);
    }

    let mut grabs = plan::pick_episodes(&candidates, season, missing);
    let mut remaining = missing.clone();
    for (_, episodes) in &grabs {
        episodes.iter().for_each(|e| {
            remaining.remove(e);
        });
    }
    let to_search: Vec<u32> = remaining
        .iter()
        .copied()
        .take(MAX_EPISODE_SEARCHES)
        .collect();
    for episode in to_search {
        if !remaining.contains(&episode) {
            continue;
        }
        let found = search_releases(inner, request, Some(season), Some(episode)).await?;
        for (release, episodes) in plan::pick_episodes(&found, season, &remaining) {
            episodes.iter().for_each(|e| {
                remaining.remove(e);
            });
            grabs.push((release, episodes));
        }
    }
    if !remaining.is_empty()
        && let Some(pack) = plan::best_pack(&candidates, season)
    {
        grabs.push((pack.clone(), remaining.iter().copied().collect()));
    }
    Ok(grabs)
}

async fn search_releases(
    inner: &Inner,
    request: &StoredRequest,
    season: Option<u32>,
    episode: Option<u32>,
) -> Result<Vec<TitleRelease>, AppError> {
    let (titles, year) = inner.search_titles(request.key).await?;
    let query = TitleQuery {
        kind: Some(request.key.kind),
        titles: if titles.is_empty() {
            vec![request.title.clone()]
        } else {
            titles
        },
        year: year.or(request.year),
        season,
        episode,
        quality: request.quality,
    };
    let integrations = inner.deps.integrations.current();
    let settings = inner.deps.settings.get();
    search::search_title(&integrations, &inner.deps.releases, &settings.media, &query).await
}

/// The hand-picked release of a request, once; expired ones are ignored.
async fn take_manual_release(
    inner: &Inner,
    request: &StoredRequest,
) -> Result<Option<CachedRelease>, AppError> {
    let Some(release_id) = &request.release_id else {
        return Ok(None);
    };
    store::clear_release_id(&inner.deps.pool, request.id).await?;
    Ok(inner.deps.releases.get(release_id))
}

async fn grab(
    inner: &Inner,
    request: &StoredRequest,
    release: &TitleRelease,
    assigned: Coverage,
) -> Result<(), AppError> {
    let cached = inner
        .deps
        .releases
        .get(&release.release.release_id)
        .ok_or(AppError::ReleaseExpired)?;
    grab_cached(
        inner,
        request,
        &cached,
        Some(release.release.size),
        assigned,
    )
    .await
}

/// Adds a release to qBittorrent and records it as serving `assigned` for the request.
async fn grab_cached(
    inner: &Inner,
    request: &StoredRequest,
    release: &CachedRelease,
    size: Option<u64>,
    assigned: Coverage,
) -> Result<(), AppError> {
    let integrations = inner.deps.integrations.current();
    let settings = inner.deps.settings.get();
    let media_type = Some(request.key.kind.media_type());
    let id = downloads::add(&integrations, &settings, release, media_type, None).await?;
    tracing::info!("request {}: grabbed {}", request.id, release.title);
    store::insert_torrent(
        &inner.deps.pool,
        &StoredTorrent {
            request_id: request.id,
            id,
            release_title: release.title.clone(),
            quality: quality::parse(&release.title),
            size: size.unwrap_or_default(),
            coverage: assigned,
            state: TorrentState::Downloading,
            error: None,
        },
    )
    .await?;
    inner.tracker.notify_one();
    Ok(())
}

/// Follows downloading torrents. Subscribes to the download monitor (which then polls
/// qBittorrent) only while there is something to follow.
pub(super) async fn track_downloads(inner: Arc<Inner>) {
    let mut progress = ProgressLog::default();
    loop {
        match store::torrents_in_state(&inner.deps.pool, TorrentState::Downloading).await {
            Ok(torrents) if !torrents.is_empty() => {}
            Ok(_) => {
                tokio::select! {
                    _ = inner.tracker.notified() => {}
                    _ = tokio::time::sleep(TRACK_INTERVAL) => {}
                }
                continue;
            }
            Err(err) => {
                tracing::warn!("download tracker: {err}");
                tokio::time::sleep(TRACK_INTERVAL).await;
                continue;
            }
        }

        let mut snapshots = inner.deps.monitor.subscribe();
        // The monitor may have been idle: start from a fresh list.
        let mut snapshot: Snapshot = match downloads::list(&inner.deps.integrations.current()).await
        {
            Ok(downloads) => Arc::new(downloads),
            Err(_) => snapshots.borrow_and_update().clone(),
        };
        loop {
            match follow(&inner, &snapshot, &mut progress).await {
                Ok(true) => {}
                Ok(false) => break,
                Err(err) => tracing::warn!("download tracker: {err}"),
            }
            tokio::select! {
                changed = snapshots.changed() => if changed.is_err() { break },
                _ = inner.tracker.notified() => {}
                _ = tokio::time::sleep(TRACK_INTERVAL) => {}
            }
            snapshot = snapshots.borrow_and_update().clone();
        }
    }
}

/// Handles one snapshot; `false` once nothing is downloading.
async fn follow(
    inner: &Arc<Inner>,
    snapshot: &[Download],
    progress: &mut ProgressLog,
) -> Result<bool, AppError> {
    let pool = &inner.deps.pool;
    let tracked = store::torrents_in_state(pool, TorrentState::Downloading).await?;
    if tracked.is_empty() {
        return Ok(false);
    }
    let integrations = inner.deps.integrations.current();
    for torrent in &tracked {
        match snapshot.iter().find(|download| download.id == torrent.id) {
            // The snapshot can predate a torrent added a moment ago: ask before giving up.
            None => match downloads::get(&integrations, &torrent.id).await {
                Err(AppError::NotFound(_)) => {
                    fail_torrent(inner, torrent, "the torrent was removed from qBittorrent")
                        .await?;
                }
                Ok(download) if is_complete(&download) => {
                    start_import(inner, torrent, download).await?
                }
                _ => {}
            },
            Some(download) if download.state == DownloadState::Error => {
                fail_torrent(
                    inner,
                    torrent,
                    "qBittorrent reports an error for this torrent",
                )
                .await?;
            }
            Some(download) if is_complete(download) => {
                start_import(inner, torrent, download.clone()).await?;
            }
            Some(_) => {}
        }
    }

    let mut requests: Vec<RequestId> = tracked.iter().map(|torrent| torrent.request_id).collect();
    requests.dedup();
    for request_id in requests {
        publish_progress(inner, request_id, snapshot, progress).await?;
    }
    Ok(true)
}

fn is_complete(download: &Download) -> bool {
    download.progress >= 1.0
        && matches!(
            download.state,
            DownloadState::Seeding | DownloadState::Completed
        )
}

/// Remembers the last progress sent per request, to send only changes.
#[derive(Default)]
struct ProgressLog(HashMap<RequestId, (u64, u64, u64)>);

async fn publish_progress(
    inner: &Inner,
    request_id: RequestId,
    snapshot: &[Download],
    log: &mut ProgressLog,
) -> Result<(), AppError> {
    let torrents = store::torrents_of(&inner.deps.pool, request_id).await?;
    let (done, total) = transfer_totals(&torrents, snapshot);
    let live: Vec<&Download> = torrents
        .iter()
        .filter(|torrent| torrent.state == TorrentState::Downloading)
        .filter_map(|torrent| snapshot.iter().find(|download| download.id == torrent.id))
        .collect();
    let dl_speed = live.iter().map(|download| download.dl_speed).sum();
    let eta_secs = live
        .iter()
        .map(|download| download.eta_secs)
        .try_fold(0, |longest: u64, eta| eta.map(|eta| longest.max(eta)));
    let current = (done, total, dl_speed);
    if log.0.get(&request_id) == Some(&current) {
        return Ok(());
    }
    log.0.insert(request_id, current);
    let Some(request) = store::get(&inner.deps.pool, request_id).await? else {
        return Ok(());
    };
    inner.deps.events.publish(Event::RequestProgress {
        request_id,
        key: request.key.to_string(),
        progress: if total == 0 {
            0.0
        } else {
            (done as f64 / total as f64).clamp(0.0, 1.0)
        },
        dl_speed,
        eta_secs: eta_secs.filter(|_| !live.is_empty()),
    });
    Ok(())
}

async fn fail_torrent(
    inner: &Inner,
    torrent: &StoredTorrent,
    reason: &str,
) -> Result<(), AppError> {
    let pool = &inner.deps.pool;
    store::set_torrent_state(
        pool,
        torrent.request_id,
        &torrent.id,
        TorrentState::Failed,
        Some(reason),
    )
    .await?;
    let others_active = store::torrents_of(pool, torrent.request_id)
        .await?
        .iter()
        .any(is_transferring);
    if others_active {
        inner.publish_updated(torrent.request_id).await;
        Ok(())
    } else {
        inner
            .set_state(torrent.request_id, RequestState::Failed, Some(reason))
            .await
    }
}

/// Marks the torrent as importing (so later snapshots skip it) and imports it in a task.
async fn start_import(
    inner: &Arc<Inner>,
    torrent: &StoredTorrent,
    download: Download,
) -> Result<(), AppError> {
    let pool = &inner.deps.pool;
    store::set_torrent_state(
        pool,
        torrent.request_id,
        &torrent.id,
        TorrentState::Importing,
        None,
    )
    .await?;
    let paused = store::get(pool, torrent.request_id)
        .await?
        .is_some_and(|request| request.state == RequestState::Paused);
    if !paused {
        inner
            .set_state(torrent.request_id, RequestState::Importing, None)
            .await?;
    }
    let inner = inner.clone();
    let torrent = torrent.clone();
    tokio::spawn(async move {
        if let Err(err) = import_torrent(&inner, &torrent, &download).await {
            tracing::warn!("import of {} failed: {err}", torrent.release_title);
            let reason = format!("import failed: {err}");
            let _ = store::set_torrent_state(
                &inner.deps.pool,
                torrent.request_id,
                &torrent.id,
                TorrentState::ImportFailed,
                Some(&reason),
            )
            .await;
            let _ = inner
                .set_state(torrent.request_id, RequestState::Failed, Some(&reason))
                .await;
        }
    });
    Ok(())
}

async fn import_torrent(
    inner: &Inner,
    torrent: &StoredTorrent,
    download: &Download,
) -> Result<(), AppError> {
    let pool = &inner.deps.pool;
    let Some(request) = store::get(pool, torrent.request_id).await? else {
        return Ok(());
    };
    let integrations = inner.deps.integrations.current();
    let settings = inner.deps.settings.get();
    let title = ImportTitle {
        kind: request.key.kind,
        tmdb_id: request.key.tmdb_id,
        title: &request.title,
        year: request.year,
        quality: torrent.quality.label(),
    };
    let imported = import::import_download(
        &integrations,
        &settings.library,
        &title,
        &torrent.coverage,
        download,
    )
    .await?;
    for file in &imported {
        store::insert_file(
            pool,
            &StoredFile {
                request_id: request.id,
                info_hash: torrent.id.to_string(),
                source: file.source.display().to_string(),
                destination: file.destination.display().to_string(),
                season: file.season,
                episode: file.episode,
            },
        )
        .await?;
    }
    store::set_torrent_state(pool, request.id, &torrent.id, TorrentState::Imported, None).await?;
    tracing::info!(
        "request {}: imported {} file(s) from {}",
        request.id,
        imported.len(),
        torrent.release_title
    );

    if settings.library.import_mode == ImportMode::Move
        && store::torrent_users(pool, &torrent.id).await? <= 1
    {
        // The videos moved away; what is left (samples, nfo) goes with the torrent.
        if let Err(err) = downloads::delete(&integrations, &torrent.id, true).await {
            tracing::warn!(
                "cannot remove {} after moving its files: {err}",
                torrent.release_title
            );
        }
    }
    if let Ok(jellyfin) = integrations.jellyfin()
        && let Err(err) = jellyfin.refresh_library().await
    {
        tracing::warn!("cannot ask Jellyfin to rescan: {err}");
    }
    inner.deps.library.sync_after_import();
    inner.enqueue(request.id);
    Ok(())
}
