//! Paired devices in SQLite.

use sqlx::SqlitePool;

use super::DeviceId;
use crate::error::AppError;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeviceRow {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
}

pub async fn insert(
    pool: &SqlitePool,
    name: &str,
    key_hash: &str,
    now: i64,
) -> Result<DeviceId, AppError> {
    let id = sqlx::query_scalar(
        "INSERT INTO devices (name, key_hash, created_at) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(name)
    .bind(key_hash)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<DeviceRow>, AppError> {
    let rows = sqlx::query_as(
        "SELECT id, name, created_at, last_seen_at FROM devices ORDER BY created_at, id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Every device's key hash, to check keys without a query per request.
pub async fn key_hashes(pool: &SqlitePool) -> Result<Vec<(String, DeviceId)>, AppError> {
    let rows = sqlx::query_as("SELECT key_hash, id FROM devices")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// True when the device existed.
pub async fn delete(pool: &SqlitePool, id: DeviceId) -> Result<bool, AppError> {
    let result = sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_last_seen(pool: &SqlitePool, id: DeviceId, now: i64) -> Result<(), AppError> {
    sqlx::query("UPDATE devices SET last_seen_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
