//! Requests, their torrents and imported files in SQLite.

use sqlx::SqlitePool;

use super::model::{RequestId, RequestState, Scope, TorrentState};
use crate::clock;
use crate::domain::{DownloadId, ReleaseId};
use crate::error::AppError;
use crate::quality::Quality;
use crate::release_name::Coverage;
use crate::titles::{TitleKey, TitleKind};

#[derive(Debug, Clone, PartialEq)]
pub struct StoredRequest {
    pub id: RequestId,
    pub key: TitleKey,
    pub title: String,
    pub year: Option<i32>,
    pub poster_path: Option<String>,
    pub scope: Scope,
    pub quality: Option<Quality>,
    pub release_id: Option<ReleaseId>,
    pub state: RequestState,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A request about to be stored.
#[derive(Debug, Clone)]
pub struct RequestDraft {
    pub key: TitleKey,
    pub title: String,
    pub year: Option<i32>,
    pub poster_path: Option<String>,
    pub scope: Scope,
    pub quality: Option<Quality>,
    pub release_id: Option<ReleaseId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredTorrent {
    pub request_id: RequestId,
    pub id: DownloadId,
    pub release_title: String,
    pub quality: Quality,
    pub size: u64,
    pub coverage: Coverage,
    pub state: TorrentState,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredFile {
    pub request_id: RequestId,
    pub info_hash: String,
    pub source: String,
    pub destination: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

#[derive(sqlx::FromRow)]
struct RequestRow {
    id: i64,
    kind: String,
    tmdb_id: i64,
    title: String,
    year: Option<i64>,
    poster_path: Option<String>,
    scope: String,
    quality: Option<String>,
    release_id: Option<String>,
    state: String,
    error: Option<String>,
    created_at: i64,
    updated_at: i64,
}

#[derive(sqlx::FromRow)]
struct TorrentRow {
    request_id: i64,
    info_hash: String,
    release_title: String,
    quality: String,
    size: i64,
    coverage: String,
    state: String,
    error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct FileRow {
    request_id: i64,
    info_hash: String,
    source: String,
    destination: String,
    season: Option<i64>,
    episode: Option<i64>,
}

fn corrupt(what: &str) -> AppError {
    AppError::Internal(format!("corrupt {what} in the database"))
}

impl TryFrom<RequestRow> for StoredRequest {
    type Error = AppError;

    fn try_from(row: RequestRow) -> Result<Self, AppError> {
        let kind = TitleKind::from_id(&row.kind).ok_or_else(|| corrupt("request kind"))?;
        let tmdb_id = u32::try_from(row.tmdb_id).map_err(|_| corrupt("TMDB id"))?;
        Ok(Self {
            id: row.id,
            key: TitleKey::new(kind, tmdb_id),
            title: row.title,
            year: row.year.and_then(|year| i32::try_from(year).ok()),
            poster_path: row.poster_path,
            scope: serde_json::from_str(&row.scope).map_err(|_| corrupt("request scope"))?,
            quality: row.quality.as_deref().and_then(Quality::from_id),
            release_id: row
                .release_id
                .map(|id| serde_json::from_value(serde_json::Value::String(id)))
                .transpose()
                .map_err(|_| corrupt("release id"))?,
            state: RequestState::from_id(&row.state).ok_or_else(|| corrupt("request state"))?,
            error: row.error,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

impl TryFrom<TorrentRow> for StoredTorrent {
    type Error = AppError;

    fn try_from(row: TorrentRow) -> Result<Self, AppError> {
        Ok(Self {
            request_id: row.request_id,
            id: DownloadId::try_from(row.info_hash).map_err(|_| corrupt("info-hash"))?,
            release_title: row.release_title,
            quality: Quality::from_id(&row.quality).unwrap_or(Quality::Unknown),
            size: u64::try_from(row.size).unwrap_or_default(),
            coverage: serde_json::from_str(&row.coverage)
                .map_err(|_| corrupt("torrent coverage"))?,
            state: TorrentState::from_id(&row.state).ok_or_else(|| corrupt("torrent state"))?,
            error: row.error,
        })
    }
}

impl From<FileRow> for StoredFile {
    fn from(row: FileRow) -> Self {
        let number = |value: Option<i64>| value.and_then(|n| u32::try_from(n).ok());
        Self {
            request_id: row.request_id,
            info_hash: row.info_hash,
            source: row.source,
            destination: row.destination,
            season: number(row.season),
            episode: number(row.episode),
        }
    }
}

fn to_requests(rows: Vec<RequestRow>) -> Result<Vec<StoredRequest>, AppError> {
    rows.into_iter().map(StoredRequest::try_from).collect()
}

fn to_torrents(rows: Vec<TorrentRow>) -> Result<Vec<StoredTorrent>, AppError> {
    rows.into_iter().map(StoredTorrent::try_from).collect()
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(|err| AppError::Internal(format!("cannot encode: {err}")))
}

/// Stores a new request; `None` when the same title and scope is already requested.
pub async fn insert(
    pool: &SqlitePool,
    draft: &RequestDraft,
) -> Result<Option<RequestId>, AppError> {
    let now = clock::now_unix();
    let id: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO requests (kind, tmdb_id, title, year, poster_path, scope, scope_key, quality, release_id, state, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (kind, tmdb_id, scope_key) DO NOTHING
         RETURNING id",
    )
    .bind(draft.key.kind.id())
    .bind(i64::from(draft.key.tmdb_id))
    .bind(&draft.title)
    .bind(draft.year)
    .bind(&draft.poster_path)
    .bind(to_json(&draft.scope)?)
    .bind(draft.scope.key())
    .bind(draft.quality.map(Quality::id))
    .bind(draft.release_id.as_ref().map(ToString::to_string))
    .bind(RequestState::Searching.id())
    .bind(now)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    Ok(id.map(|(id,)| id))
}

pub async fn find_by_scope(
    pool: &SqlitePool,
    key: TitleKey,
    scope: &Scope,
) -> Result<Option<StoredRequest>, AppError> {
    let row: Option<RequestRow> = sqlx::query_as(
        "SELECT id, kind, tmdb_id, title, year, poster_path, scope, quality, release_id, state, error, created_at, updated_at
         FROM requests WHERE kind = ? AND tmdb_id = ? AND scope_key = ?",
    )
    .bind(key.kind.id())
    .bind(i64::from(key.tmdb_id))
    .bind(scope.key())
    .fetch_optional(pool)
    .await?;
    row.map(StoredRequest::try_from).transpose()
}

pub async fn get(pool: &SqlitePool, id: RequestId) -> Result<Option<StoredRequest>, AppError> {
    let row: Option<RequestRow> = sqlx::query_as(
        "SELECT id, kind, tmdb_id, title, year, poster_path, scope, quality, release_id, state, error, created_at, updated_at
         FROM requests WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.map(StoredRequest::try_from).transpose()
}

/// A page of requests, newest first, optionally in one state; with the total count.
pub async fn list(
    pool: &SqlitePool,
    state: Option<RequestState>,
    limit: i64,
    offset: i64,
) -> Result<(Vec<StoredRequest>, u32), AppError> {
    let state = state.map(RequestState::id);
    let rows: Vec<RequestRow> = sqlx::query_as(
        "SELECT id, kind, tmdb_id, title, year, poster_path, scope, quality, release_id, state, error, created_at, updated_at
         FROM requests WHERE (?1 IS NULL OR state = ?1)
         ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3",
    )
    .bind(state)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    let (total,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM requests WHERE (?1 IS NULL OR state = ?1)")
            .bind(state)
            .fetch_one(pool)
            .await?;
    Ok((to_requests(rows)?, u32::try_from(total).unwrap_or(u32::MAX)))
}

pub async fn in_states(
    pool: &SqlitePool,
    states: &[RequestState],
) -> Result<Vec<StoredRequest>, AppError> {
    let ids: Vec<&str> = states.iter().map(|state| state.id()).collect();
    let rows: Vec<RequestRow> = sqlx::query_as(
        "SELECT id, kind, tmdb_id, title, year, poster_path, scope, quality, release_id, state, error, created_at, updated_at
         FROM requests WHERE state IN (SELECT value FROM json_each(?)) ORDER BY id",
    )
    .bind(to_json(&ids)?)
    .fetch_all(pool)
    .await?;
    to_requests(rows)
}

pub async fn for_title(pool: &SqlitePool, key: TitleKey) -> Result<Vec<StoredRequest>, AppError> {
    let rows: Vec<RequestRow> = sqlx::query_as(
        "SELECT id, kind, tmdb_id, title, year, poster_path, scope, quality, release_id, state, error, created_at, updated_at
         FROM requests WHERE kind = ? AND tmdb_id = ? ORDER BY id",
    )
    .bind(key.kind.id())
    .bind(i64::from(key.tmdb_id))
    .fetch_all(pool)
    .await?;
    to_requests(rows)
}

pub async fn set_state(
    pool: &SqlitePool,
    id: RequestId,
    state: RequestState,
    error: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query("UPDATE requests SET state = ?, error = ?, updated_at = ? WHERE id = ?")
        .bind(state.id())
        .bind(error)
        .bind(clock::now_unix())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The hand-picked release is used once; later searches pick automatically.
pub async fn clear_release_id(pool: &SqlitePool, id: RequestId) -> Result<(), AppError> {
    sqlx::query("UPDATE requests SET release_id = NULL WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: RequestId) -> Result<(), AppError> {
    sqlx::query("DELETE FROM requests WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_torrent(pool: &SqlitePool, torrent: &StoredTorrent) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO request_torrents (request_id, info_hash, release_title, quality, size, coverage, state, error, added_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (request_id, info_hash) DO UPDATE SET
             coverage = excluded.coverage, state = excluded.state, error = NULL",
    )
    .bind(torrent.request_id)
    .bind(torrent.id.as_str())
    .bind(&torrent.release_title)
    .bind(torrent.quality.id())
    .bind(i64::try_from(torrent.size).unwrap_or(i64::MAX))
    .bind(to_json(&torrent.coverage)?)
    .bind(torrent.state.id())
    .bind(&torrent.error)
    .bind(clock::now_unix())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn torrents_of(
    pool: &SqlitePool,
    request_id: RequestId,
) -> Result<Vec<StoredTorrent>, AppError> {
    let rows: Vec<TorrentRow> = sqlx::query_as(
        "SELECT request_id, info_hash, release_title, quality, size, coverage, state, error
         FROM request_torrents WHERE request_id = ? ORDER BY added_at, info_hash",
    )
    .bind(request_id)
    .fetch_all(pool)
    .await?;
    to_torrents(rows)
}

pub async fn torrents_in_state(
    pool: &SqlitePool,
    state: TorrentState,
) -> Result<Vec<StoredTorrent>, AppError> {
    let rows: Vec<TorrentRow> = sqlx::query_as(
        "SELECT request_id, info_hash, release_title, quality, size, coverage, state, error
         FROM request_torrents WHERE state = ? ORDER BY request_id, added_at",
    )
    .bind(state.id())
    .fetch_all(pool)
    .await?;
    to_torrents(rows)
}

/// Torrents of every request of a title, to know which episodes are already on their way.
pub async fn torrents_for_title(
    pool: &SqlitePool,
    key: TitleKey,
) -> Result<Vec<StoredTorrent>, AppError> {
    let rows: Vec<TorrentRow> = sqlx::query_as(
        "SELECT t.request_id, t.info_hash, t.release_title, t.quality, t.size, t.coverage, t.state, t.error
         FROM request_torrents t JOIN requests r ON r.id = t.request_id
         WHERE r.kind = ? AND r.tmdb_id = ?",
    )
    .bind(key.kind.id())
    .bind(i64::from(key.tmdb_id))
    .fetch_all(pool)
    .await?;
    to_torrents(rows)
}

/// How many requests use a torrent (a pack can serve several).
pub async fn torrent_users(pool: &SqlitePool, id: &DownloadId) -> Result<u32, AppError> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM request_torrents WHERE info_hash = ?")
            .bind(id.as_str())
            .fetch_one(pool)
            .await?;
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

pub async fn set_torrent_state(
    pool: &SqlitePool,
    request_id: RequestId,
    id: &DownloadId,
    state: TorrentState,
    error: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE request_torrents SET state = ?, error = ? WHERE request_id = ? AND info_hash = ?",
    )
    .bind(state.id())
    .bind(error)
    .bind(request_id)
    .bind(id.as_str())
    .execute(pool)
    .await?;
    Ok(())
}

/// Imports cut short by a restart start over.
pub async fn restart_interrupted_imports(pool: &SqlitePool) -> Result<(), AppError> {
    sqlx::query("UPDATE request_torrents SET state = ? WHERE state = ?")
        .bind(TorrentState::Downloading.id())
        .bind(TorrentState::Importing.id())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_file(pool: &SqlitePool, file: &StoredFile) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO imported_files (request_id, info_hash, source, destination, season, episode, imported_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(file.request_id)
    .bind(&file.info_hash)
    .bind(&file.source)
    .bind(&file.destination)
    .bind(file.season.map(i64::from))
    .bind(file.episode.map(i64::from))
    .bind(clock::now_unix())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn files_of(
    pool: &SqlitePool,
    request_id: RequestId,
) -> Result<Vec<StoredFile>, AppError> {
    let rows: Vec<FileRow> = sqlx::query_as(
        "SELECT request_id, info_hash, source, destination, season, episode
         FROM imported_files WHERE request_id = ? ORDER BY season, episode, id",
    )
    .bind(request_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(StoredFile::from).collect())
}

/// Files imported for any request of a title.
pub async fn files_for_title(
    pool: &SqlitePool,
    key: TitleKey,
) -> Result<Vec<StoredFile>, AppError> {
    let rows: Vec<FileRow> = sqlx::query_as(
        "SELECT f.request_id, f.info_hash, f.source, f.destination, f.season, f.episode
         FROM imported_files f JOIN requests r ON r.id = f.request_id
         WHERE r.kind = ? AND r.tmdb_id = ?",
    )
    .bind(key.kind.id())
    .bind(i64::from(key.tmdb_id))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(StoredFile::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool(name: &str) -> (SqlitePool, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("fetcherr-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (crate::db::open(&dir).await.unwrap(), dir)
    }

    fn draft(scope: Scope) -> RequestDraft {
        RequestDraft {
            key: TitleKey::new(TitleKind::Series, 1399),
            title: "Game of Thrones".into(),
            year: Some(2011),
            poster_path: Some("/p.jpg".into()),
            scope,
            quality: Some(Quality::Web1080p),
            release_id: None,
        }
    }

    #[tokio::test]
    async fn stores_requests_once_per_scope_and_pages_them() {
        let (pool, dir) = pool("requests").await;
        let first = insert(&pool, &draft(Scope::Season { season: 1 }))
            .await
            .unwrap();
        let again = insert(&pool, &draft(Scope::Season { season: 1 }))
            .await
            .unwrap();
        let second = insert(&pool, &draft(Scope::Season { season: 2 }))
            .await
            .unwrap();
        assert!(first.is_some() && second.is_some());
        assert_eq!(again, None);

        let stored = get(&pool, first.unwrap()).await.unwrap().unwrap();
        assert_eq!(stored.scope, Scope::Season { season: 1 });
        assert_eq!(stored.quality, Some(Quality::Web1080p));
        assert_eq!(stored.state, RequestState::Searching);

        set_state(&pool, stored.id, RequestState::NotFound, Some("nothing"))
            .await
            .unwrap();
        let (page, total) = list(&pool, Some(RequestState::Searching), 20, 0)
            .await
            .unwrap();
        assert_eq!((page.len(), total), (1, 1));
        let (_, total) = list(&pool, None, 20, 0).await.unwrap();
        assert_eq!(total, 2);
        let waiting = in_states(&pool, &[RequestState::NotFound]).await.unwrap();
        assert_eq!(waiting[0].error.as_deref(), Some("nothing"));
        drop(pool);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn tracks_torrents_and_files_per_request() {
        let (pool, dir) = pool("torrents").await;
        let id = insert(&pool, &draft(Scope::Season { season: 1 }))
            .await
            .unwrap()
            .unwrap();
        let hash = DownloadId::try_from("a".repeat(40)).unwrap();
        let torrent = StoredTorrent {
            request_id: id,
            id: hash.clone(),
            release_title: "Show.S01.1080p".into(),
            quality: Quality::Web1080p,
            size: 10,
            coverage: Coverage::Episodes {
                season: 1,
                episodes: vec![1, 2],
            },
            state: TorrentState::Importing,
            error: None,
        };
        insert_torrent(&pool, &torrent).await.unwrap();
        restart_interrupted_imports(&pool).await.unwrap();
        let downloading = torrents_in_state(&pool, TorrentState::Downloading)
            .await
            .unwrap();
        assert_eq!(downloading[0].coverage, torrent.coverage);
        assert_eq!(torrent_users(&pool, &hash).await.unwrap(), 1);

        insert_file(
            &pool,
            &StoredFile {
                request_id: id,
                info_hash: hash.to_string(),
                source: "/d/a.mkv".into(),
                destination: "/m/a.mkv".into(),
                season: Some(1),
                episode: Some(1),
            },
        )
        .await
        .unwrap();
        let key = TitleKey::new(TitleKind::Series, 1399);
        assert_eq!(files_for_title(&pool, key).await.unwrap().len(), 1);

        delete(&pool, id).await.unwrap();
        assert!(torrents_of(&pool, id).await.unwrap().is_empty());
        assert!(files_of(&pool, id).await.unwrap().is_empty());
        drop(pool);
        let _ = std::fs::remove_dir_all(dir);
    }
}
