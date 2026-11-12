//! The SQLite database (`<data dir>/fetcherr.db`): requests, their torrents and imports, and
//! the ratings cache. Migrations in `migrations/` are embedded and run at startup.

use std::path::Path;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::error::AppError;

const FILE_NAME: &str = "fetcherr.db";

pub async fn open(data_dir: &Path) -> Result<SqlitePool, AppError> {
    tokio::fs::create_dir_all(data_dir).await.map_err(|err| {
        AppError::Internal(format!("cannot create {}: {err}", data_dir.display()))
    })?;
    let options = SqliteConnectOptions::new()
        .filename(data_dir.join(FILE_NAME))
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|err| AppError::Internal(format!("database migration failed: {err}")))?;
    Ok(pool)
}
