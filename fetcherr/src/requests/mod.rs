//! Requests: a title and scope someone wants. Fetcherr finds releases in the background, follows
//! their downloads and imports them; see [`worker`] for that lifecycle.

mod model;
mod plan;
mod store;
mod worker;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use sqlx::SqlitePool;
use tokio::sync::{Notify, mpsc};

use crate::domain::{Download, PAGE_SIZE, Page, PageQuery};
use crate::downloads::{self, DownloadMonitor};
use crate::error::AppError;
use crate::events::{Event, EventBus};
use crate::integrations::SharedIntegrations;
use crate::library::Library;
use crate::metadata;
use crate::release_name;
use crate::releases::ReleaseCache;
use crate::search::{self, TitleQuery, TitleRelease};
use crate::settings::SettingsStore;
use crate::titles::{ImageKind, Images, TitleKey, TitleKind};

pub use model::{
    ImportedFileView, NewRequest, RequestDetail, RequestId, RequestState, RequestSummary,
    RequestTorrent, Scope, TorrentState,
};
use store::{RequestDraft, StoredRequest, StoredTorrent};

/// What the request service needs from the rest of the app.
pub struct RequestDeps {
    pub pool: SqlitePool,
    pub integrations: Arc<SharedIntegrations>,
    pub settings: Arc<SettingsStore>,
    pub releases: Arc<ReleaseCache>,
    pub monitor: DownloadMonitor,
    pub library: Library,
    pub events: EventBus,
}

#[derive(Clone)]
pub struct Requests {
    inner: Arc<Inner>,
}

struct Inner {
    deps: RequestDeps,
    queue: mpsc::UnboundedSender<RequestId>,
    queued: Mutex<HashSet<RequestId>>,
    /// Wakes the download tracker when a torrent was added or resumed.
    tracker: Notify,
}

/// An unfinished (or available) request, as the catalog needs it for availability.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleRequest {
    pub scope: Scope,
    pub state: RequestState,
}

impl Requests {
    pub async fn start(deps: RequestDeps) -> Result<Self, AppError> {
        store::restart_interrupted_imports(&deps.pool).await?;
        let (queue, receiver) = mpsc::unbounded_channel();
        let inner = Arc::new(Inner {
            deps,
            queue,
            queued: Mutex::new(HashSet::new()),
            tracker: Notify::new(),
        });
        tokio::spawn(worker::run_queue(inner.clone(), receiver));
        tokio::spawn(worker::track_downloads(inner.clone()));
        tokio::spawn(worker::retry_periodically(inner.clone()));
        for request in store::in_states(
            &inner.deps.pool,
            &[RequestState::Searching, RequestState::Importing],
        )
        .await?
        {
            inner.enqueue(request.id);
        }
        Ok(Self { inner })
    }

    /// Creates a request (or returns the existing one for the same title and scope, with
    /// `false`). The release search runs in the background.
    pub async fn create(&self, input: NewRequest) -> Result<(RequestDetail, bool), AppError> {
        let key = TitleKey::new(input.media_type, input.tmdb_id);
        let scope = input.scope.validated(key.kind)?;
        // Fail now rather than after the download, when the import would.
        self.inner
            .deps
            .settings
            .get()
            .library
            .require_folder(key.kind)?;
        let (title, year, poster_path) = self.inner.describe(key, &scope).await?;
        if let Some(release_id) = &input.release_id {
            let cached = self
                .inner
                .deps
                .releases
                .get(release_id)
                .ok_or(AppError::ReleaseExpired)?;
            if !scope.accepts(&release_name::parse(&cached.title).coverage) {
                return Err(AppError::Invalid(format!(
                    "release '{}' does not contain what is requested",
                    cached.title
                )));
            }
        }
        let draft = RequestDraft {
            key,
            title,
            year,
            poster_path,
            scope: scope.clone(),
            quality: input.quality,
            release_id: input.release_id,
        };
        let pool = &self.inner.deps.pool;
        match store::insert(pool, &draft).await? {
            Some(id) => {
                self.inner.publish_updated(id).await;
                self.inner.enqueue(id);
                Ok((self.get(id).await?, true))
            }
            None => {
                let existing = store::find_by_scope(pool, key, &scope)
                    .await?
                    .ok_or_else(|| AppError::Internal("request vanished".into()))?;
                Ok((self.get(existing.id).await?, false))
            }
        }
    }

    pub async fn list(
        &self,
        state: Option<RequestState>,
        page: PageQuery,
    ) -> Result<Page<RequestSummary>, AppError> {
        let (requests, total) = store::list(
            &self.inner.deps.pool,
            state,
            i64::from(PAGE_SIZE),
            page.offset(),
        )
        .await?;
        let mut items = Vec::with_capacity(requests.len());
        for request in &requests {
            items.push(self.inner.summarize(request).await?);
        }
        Ok(Page::of(page.page(), total, items))
    }

    pub async fn get(&self, id: RequestId) -> Result<RequestDetail, AppError> {
        let request = self.inner.load(id).await?;
        let pool = &self.inner.deps.pool;
        let torrents = store::torrents_of(pool, id).await?;
        let files = store::files_of(pool, id).await?;
        let snapshot = self.inner.deps.monitor.latest();
        Ok(RequestDetail {
            summary: summarize(&request, &torrents, &snapshot),
            torrents: torrents
                .iter()
                .map(|torrent| torrent_view(torrent, &snapshot))
                .collect(),
            files: files
                .into_iter()
                .map(|file| ImportedFileView {
                    path: file.destination,
                    season: file.season,
                    episode: file.episode,
                })
                .collect(),
        })
    }

    /// Every request of a title, oldest first.
    pub async fn for_title(&self, key: TitleKey) -> Result<Vec<RequestSummary>, AppError> {
        let mut summaries = Vec::new();
        for request in store::for_title(&self.inner.deps.pool, key).await? {
            summaries.push(self.inner.summarize(&request).await?);
        }
        Ok(summaries)
    }

    /// What a new event viewer starts from: the given requests, the requests of the given
    /// titles, or (with neither) every unfinished request.
    pub async fn snapshot(
        &self,
        ids: Option<&HashSet<RequestId>>,
        keys: Option<&HashSet<TitleKey>>,
    ) -> Result<Vec<RequestSummary>, AppError> {
        let pool = &self.inner.deps.pool;
        let mut requests = Vec::new();
        if ids.is_none() && keys.is_none() {
            let active: Vec<RequestState> = RequestState::ALL
                .into_iter()
                .filter(|s| s.is_active())
                .collect();
            requests = store::in_states(pool, &active).await?;
        }
        for id in ids.into_iter().flatten() {
            requests.extend(store::get(pool, *id).await?);
        }
        for key in keys.into_iter().flatten() {
            requests.extend(store::for_title(pool, *key).await?);
        }
        requests.sort_by_key(|request| request.id);
        requests.dedup_by_key(|request| request.id);
        let mut summaries = Vec::with_capacity(requests.len());
        for request in &requests {
            summaries.push(self.inner.summarize(request).await?);
        }
        Ok(summaries)
    }

    /// Requests that are not failed, by title.
    pub async fn by_title(&self) -> Result<HashMap<TitleKey, Vec<TitleRequest>>, AppError> {
        let states: Vec<RequestState> = RequestState::ALL
            .into_iter()
            .filter(|state| *state != RequestState::Failed)
            .collect();
        let mut by_title: HashMap<TitleKey, Vec<TitleRequest>> = HashMap::new();
        for request in store::in_states(&self.inner.deps.pool, &states).await? {
            by_title.entry(request.key).or_default().push(TitleRequest {
                scope: request.scope,
                state: request.state,
            });
        }
        Ok(by_title)
    }

    /// Removes a request and its torrents; with `delete_files`, also their data and the files
    /// imported into the library.
    pub async fn delete(&self, id: RequestId, delete_files: bool) -> Result<(), AppError> {
        let request = self.inner.load(id).await?;
        let pool = &self.inner.deps.pool;
        let integrations = self.inner.deps.integrations.current();
        for torrent in store::torrents_of(pool, id).await? {
            if store::torrent_users(pool, &torrent.id).await? > 1 {
                continue;
            }
            match downloads::delete(&integrations, &torrent.id, delete_files).await {
                Ok(()) | Err(AppError::NotFound(_) | AppError::NotConfigured(_)) => {}
                Err(err) => return Err(err),
            }
        }
        if delete_files {
            // Movie files sit in a title folder; episodes in a season folder inside it.
            let folders = match request.key.kind {
                TitleKind::Movie => 1,
                TitleKind::Series => 2,
            };
            for file in store::files_of(pool, id).await? {
                remove_library_file(Path::new(&file.destination), folders).await;
            }
            self.inner.deps.library.sync_now();
        }
        store::delete(pool, id).await?;
        self.inner.deps.events.publish(Event::RequestDeleted {
            request_id: id,
            key: request.key.to_string(),
        });
        Ok(())
    }

    pub async fn pause(&self, id: RequestId) -> Result<RequestDetail, AppError> {
        let request = self.inner.load(id).await?;
        if request.state != RequestState::Downloading {
            return Err(AppError::Conflict(
                "only a downloading request can be paused".into(),
            ));
        }
        let integrations = self.inner.deps.integrations.current();
        for torrent in self.inner.downloading_torrents(id).await? {
            downloads::pause(&integrations, &torrent.id).await?;
        }
        self.inner.set_state(id, RequestState::Paused, None).await?;
        self.get(id).await
    }

    pub async fn resume(&self, id: RequestId) -> Result<RequestDetail, AppError> {
        let request = self.inner.load(id).await?;
        if request.state != RequestState::Paused {
            return Err(AppError::Conflict(
                "only a paused request can be resumed".into(),
            ));
        }
        let integrations = self.inner.deps.integrations.current();
        for torrent in self.inner.downloading_torrents(id).await? {
            downloads::resume(&integrations, &torrent.id).await?;
        }
        self.inner
            .set_state(id, RequestState::Downloading, None)
            .await?;
        self.inner.tracker.notify_one();
        self.get(id).await
    }

    /// Searches again for a failed or not-found request. Downloads whose import failed are
    /// imported again (if still in qBittorrent) instead of being downloaded again.
    pub async fn retry(&self, id: RequestId) -> Result<RequestDetail, AppError> {
        let request = self.inner.load(id).await?;
        if !matches!(request.state, RequestState::Failed | RequestState::NotFound) {
            return Err(AppError::Conflict(
                "only failed or not found requests can be retried".into(),
            ));
        }
        self.inner
            .deps
            .settings
            .get()
            .library
            .require_folder(request.key.kind)?;
        let pool = &self.inner.deps.pool;
        for torrent in store::torrents_of(pool, id).await? {
            if torrent.state == TorrentState::ImportFailed {
                store::set_torrent_state(pool, id, &torrent.id, TorrentState::Downloading, None)
                    .await?;
            }
        }
        self.inner
            .set_state(id, RequestState::Searching, None)
            .await?;
        self.inner.enqueue(id);
        self.inner.tracker.notify_one();
        self.get(id).await
    }

    /// Releases of a title for the manual picker; their `release_id` can be sent with a request.
    pub async fn releases(
        &self,
        key: TitleKey,
        season: Option<u32>,
        episode: Option<u32>,
    ) -> Result<Vec<TitleRelease>, AppError> {
        if key.kind == TitleKind::Movie && (season.is_some() || episode.is_some()) {
            return Err(AppError::Invalid(
                "movies have no seasons or episodes".into(),
            ));
        }
        if episode.is_some() && season.is_none() {
            return Err(AppError::Invalid("an episode needs its season".into()));
        }
        let integrations = self.inner.deps.integrations.current();
        let (titles, year) = self.inner.search_titles(key).await?;
        let query = TitleQuery {
            kind: Some(key.kind),
            titles,
            year,
            season,
            episode,
            quality: None,
        };
        let settings = self.inner.deps.settings.get();
        search::search_title(
            &integrations,
            &self.inner.deps.releases,
            &settings.media,
            &query,
        )
        .await
    }
}

impl Inner {
    fn enqueue(&self, id: RequestId) {
        let newly_queued = self
            .queued
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id);
        if newly_queued {
            let _ = self.queue.send(id);
        }
    }

    fn dequeued(&self, id: RequestId) {
        self.queued
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&id);
    }

    async fn load(&self, id: RequestId) -> Result<StoredRequest, AppError> {
        store::get(&self.deps.pool, id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("request {id}")))
    }

    async fn downloading_torrents(&self, id: RequestId) -> Result<Vec<StoredTorrent>, AppError> {
        Ok(store::torrents_of(&self.deps.pool, id)
            .await?
            .into_iter()
            .filter(|torrent| torrent.state == TorrentState::Downloading)
            .collect())
    }

    async fn set_state(
        &self,
        id: RequestId,
        state: RequestState,
        error: Option<&str>,
    ) -> Result<(), AppError> {
        store::set_state(&self.deps.pool, id, state, error).await?;
        self.publish_updated(id).await;
        Ok(())
    }

    async fn publish_updated(&self, id: RequestId) {
        let Ok(Some(request)) = store::get(&self.deps.pool, id).await else {
            return;
        };
        if let Ok(summary) = self.summarize(&request).await {
            self.deps.events.publish(Event::RequestUpdated {
                request: Box::new(summary),
            });
        }
    }

    async fn summarize(&self, request: &StoredRequest) -> Result<RequestSummary, AppError> {
        let torrents = store::torrents_of(&self.deps.pool, request.id).await?;
        Ok(summarize(request, &torrents, &self.deps.monitor.latest()))
    }

    /// Title, year and poster of a title, checking that a requested season exists.
    async fn describe(
        &self,
        key: TitleKey,
        scope: &Scope,
    ) -> Result<(String, Option<i32>, Option<String>), AppError> {
        let integrations = self.deps.integrations.current();
        let tmdb = integrations.tmdb()?;
        let summary = match key.kind {
            TitleKind::Movie => tmdb.movie(key.tmdb_id).await?.summary,
            TitleKind::Series => {
                let series = tmdb.series(key.tmdb_id).await?;
                if let Some(season) = scope.season()
                    && !series.seasons.iter().any(|known| known.number == season)
                {
                    return Err(AppError::Invalid(format!(
                        "{} has no season {season}",
                        series.summary.title
                    )));
                }
                series.summary
            }
        };
        Ok((summary.title, summary.year, summary.poster_path))
    }

    /// The names to match release titles against, and the year.
    async fn search_titles(&self, key: TitleKey) -> Result<(Vec<String>, Option<i32>), AppError> {
        let integrations = self.deps.integrations.current();
        let tmdb = integrations.tmdb()?;
        let (summary, alternatives) = match key.kind {
            TitleKind::Movie => {
                let movie = tmdb.movie(key.tmdb_id).await?;
                (movie.summary, movie.alternative_titles)
            }
            TitleKind::Series => {
                let series = tmdb.series(key.tmdb_id).await?;
                (series.summary, series.alternative_titles)
            }
        };
        Ok((
            metadata::search_titles(&summary, &alternatives),
            summary.year,
        ))
    }
}

/// Bytes done and total over a request's torrents; imported ones count as done.
fn transfer_totals(torrents: &[StoredTorrent], snapshot: &[Download]) -> (u64, u64) {
    torrents
        .iter()
        .filter(|torrent| torrent.state.is_useful())
        .fold((0, 0), |(done, total), torrent| {
            let live = snapshot.iter().find(|download| download.id == torrent.id);
            match (torrent.state, live) {
                (TorrentState::Imported, _) => {
                    let size = live.map_or(torrent.size, |download| download.size).max(1);
                    (done + size, total + size)
                }
                (_, Some(download)) => (done + download.downloaded, total + download.size),
                (_, None) => (done, total + torrent.size),
            }
        })
}

fn summarize(
    request: &StoredRequest,
    torrents: &[StoredTorrent],
    snapshot: &[Download],
) -> RequestSummary {
    let progress = if request.state == RequestState::Available {
        1.0
    } else {
        let (done, total) = transfer_totals(torrents, snapshot);
        if total == 0 {
            0.0
        } else {
            (done as f64 / total as f64).clamp(0.0, 1.0)
        }
    };
    RequestSummary {
        id: request.id,
        key: request.key.to_string(),
        media_type: request.key.kind,
        tmdb_id: request.key.tmdb_id,
        title: request.title.clone(),
        year: request.year,
        poster: Images::tmdb(ImageKind::Poster, request.poster_path.as_deref()),
        scope: request.scope.clone(),
        quality: request.quality,
        state: request.state,
        error: request.error.clone(),
        progress,
        created_at: crate::clock::rfc3339(request.created_at),
        updated_at: crate::clock::rfc3339(request.updated_at),
    }
}

fn torrent_view(torrent: &StoredTorrent, snapshot: &[Download]) -> RequestTorrent {
    let live = snapshot
        .iter()
        .find(|download| download.id == torrent.id)
        .filter(|_| torrent.state != TorrentState::Imported);
    RequestTorrent {
        id: torrent.id.to_string(),
        release_title: torrent.release_title.clone(),
        quality: torrent.quality,
        size: live.map_or(torrent.size, |download| download.size),
        coverage: torrent.coverage.clone(),
        state: torrent.state,
        error: torrent.error.clone(),
        progress: match torrent.state {
            TorrentState::Imported | TorrentState::Importing | TorrentState::ImportFailed => 1.0,
            _ => live.map_or(0.0, |download| download.progress),
        },
        dl_speed: live.map_or(0, |download| download.dl_speed),
        eta_secs: live.and_then(|download| download.eta_secs),
        download_state: live.map(|download| download.state),
    }
}

/// Removes an imported file and up to `folders` parent folders it leaves empty, never going
/// above the title folder.
async fn remove_library_file(path: &Path, folders: usize) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            tracing::warn!("cannot delete {}: {err}", path.display());
            return;
        }
    }
    let mut folder = path.parent();
    for _ in 0..folders {
        let Some(current) = folder else { break };
        if tokio::fs::remove_dir(current).await.is_err() {
            break;
        }
        folder = current.parent();
    }
}
