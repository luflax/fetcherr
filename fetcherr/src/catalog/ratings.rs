//! OMDb ratings cached in SQLite for a day, so the daily OMDb quota survives restarts.

use sqlx::SqlitePool;

use crate::clock;
use crate::metadata::ExternalRatings;
use crate::omdb::OmdbClient;

const MAX_AGE_SECS: i64 = 24 * 60 * 60;

/// Cached ratings, else fresh ones from OMDb. Failures only log: ratings are optional.
pub async fn cached(
    pool: &SqlitePool,
    omdb: &OmdbClient,
    imdb_id: &str,
) -> Option<ExternalRatings> {
    let oldest = clock::now_unix() - MAX_AGE_SECS;
    let stored: Option<(String,)> =
        sqlx::query_as("SELECT payload FROM ratings_cache WHERE imdb_id = ? AND fetched_at >= ?")
            .bind(imdb_id)
            .bind(oldest)
            .fetch_optional(pool)
            .await
            .inspect_err(|err| tracing::warn!("ratings cache: {err}"))
            .ok()
            .flatten();
    if let Some(ratings) = stored.and_then(|(payload,)| serde_json::from_str(&payload).ok()) {
        return Some(ratings);
    }

    let ratings = match omdb.ratings(imdb_id).await {
        Ok(ratings) => ratings,
        // Unknown to OMDb: remember that it has nothing, like any other answer.
        Err(err) if err.not_found => ExternalRatings::default(),
        Err(err) => {
            tracing::warn!("{err}");
            return None;
        }
    };
    if let Ok(payload) = serde_json::to_string(&ratings) {
        let saved = sqlx::query(
            "INSERT INTO ratings_cache (imdb_id, payload, fetched_at) VALUES (?, ?, ?)
             ON CONFLICT (imdb_id) DO UPDATE SET payload = excluded.payload, fetched_at = excluded.fetched_at",
        )
        .bind(imdb_id)
        .bind(payload)
        .bind(clock::now_unix())
        .execute(pool)
        .await;
        if let Err(err) = saved {
            tracing::warn!("ratings cache: {err}");
        }
    }
    Some(ratings)
}
