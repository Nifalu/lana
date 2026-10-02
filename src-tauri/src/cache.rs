//! Data access for the on-device SQLite cache.
//!
//! Everything the Tauri commands expose lives here as plain async functions
//! over a [`SqlitePool`], so the seams are testable without a Tauri app:
//! device identity + server URL (the `settings` table), POI/station queries,
//! wholesale snapshot replacement and cache metadata.

use anyhow::Context;
use sqlx::sqlite::SqlitePool;

/// Settings-table key holding the persistent device id (UUID v4).
const DEVICE_ID_KEY: &str = "device_id";

/// Settings-table key holding the server base URL.
const SERVER_URL_KEY: &str = "server_url";

/// Server the app syncs from until another one is configured.
pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8080";

/// Returns the persistent device id, creating it (UUID v4) on first call.
/// Stored in the cache's `settings` table, so it survives restarts.
pub async fn get_or_create_device_id(pool: &SqlitePool) -> anyhow::Result<String> {
    // Insert-first: if two calls race, `ON CONFLICT DO NOTHING` keeps the
    // first winner and the read below returns it for both callers.
    sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO NOTHING")
        .bind(DEVICE_ID_KEY)
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(pool)
        .await?;
    let (id,): (String,) = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
        .bind(DEVICE_ID_KEY)
        .fetch_one(pool)
        .await?;
    Ok(id)
}

/// Returns the configured server base URL, or [`DEFAULT_SERVER_URL`] when
/// none was set yet.
pub async fn get_server_url(pool: &SqlitePool) -> anyhow::Result<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
        .bind(SERVER_URL_KEY)
        .fetch_optional(pool)
        .await?;
    Ok(row
        .map(|(url,)| url)
        .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string()))
}

/// Persists the server base URL. Rejects values that are not `http(s)` URLs.
pub async fn set_server_url(pool: &SqlitePool, url: &str) -> anyhow::Result<()> {
    let parsed = reqwest::Url::parse(url).context("server URL must be an absolute URL")?;
    anyhow::ensure!(
        parsed.scheme() == "http" || parsed.scheme() == "https",
        "server URL must use http or https"
    );
    let normalized = url.trim_end_matches('/');
    anyhow::ensure!(!normalized.is_empty(), "server URL must not be empty");
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(SERVER_URL_KEY)
    .bind(normalized)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    async fn test_pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = db::open_at(&dir.path().join("cache.sqlite3"))
            .await
            .expect("open cache");
        (dir, pool)
    }

    fn assert_uuid_v4(value: &str) {
        let id = uuid::Uuid::parse_str(value).expect("device id is a UUID");
        assert_eq!(id.get_version_num(), 4, "device id is UUID v4");
    }

    /// The device id is generated once and then stable across reopens of the
    /// same cache file (ADR 0004: client-generated UUID, persisted on
    /// device).
    #[tokio::test]
    async fn device_id_is_generated_once_and_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.sqlite3");

        let first = {
            let pool = db::open_at(&path).await.unwrap();
            let id = get_or_create_device_id(&pool).await.unwrap();
            pool.close().await;
            id
        };
        assert_uuid_v4(&first);

        let second = {
            let pool = db::open_at(&path).await.unwrap();
            let id = get_or_create_device_id(&pool).await.unwrap();
            pool.close().await;
            id
        };
        assert_eq!(first, second, "device id must not change across restarts");
    }

    /// Two different cache files carry two different device ids (no shared
    /// global state sneaking in).
    #[tokio::test]
    async fn device_ids_differ_between_devices() {
        let (_dir_a, pool_a) = test_pool().await;
        let (_dir_b, pool_b) = test_pool().await;
        let a = get_or_create_device_id(&pool_a).await.unwrap();
        let b = get_or_create_device_id(&pool_b).await.unwrap();
        assert_ne!(a, b);
    }

    /// The server URL defaults to the local demo server and round-trips a
    /// configured value.
    #[tokio::test]
    async fn server_url_defaults_then_round_trips() {
        let (_dir, pool) = test_pool().await;
        assert_eq!(
            get_server_url(&pool).await.unwrap(),
            DEFAULT_SERVER_URL,
            "default before anything is configured"
        );
        set_server_url(&pool, "http://192.168.1.42:8080")
            .await
            .expect("valid url accepted");
        assert_eq!(
            get_server_url(&pool).await.unwrap(),
            "http://192.168.1.42:8080"
        );
    }

    /// Trailing slashes are normalized so `{base}/api/v1/snapshot` never
    /// doubles a slash.
    #[tokio::test]
    async fn server_url_normalizes_trailing_slash() {
        let (_dir, pool) = test_pool().await;
        set_server_url(&pool, "http://example.com/").await.unwrap();
        assert_eq!(get_server_url(&pool).await.unwrap(), "http://example.com");
    }

    /// Only http(s) URLs are accepted – typos should fail loudly at save
    /// time, not as a confusing sync error later.
    #[tokio::test]
    async fn server_url_rejects_non_http_values() {
        let (_dir, pool) = test_pool().await;
        for bad in ["not a url", "ftp://example.com", ""] {
            assert!(
                set_server_url(&pool, bad).await.is_err(),
                "{bad:?} rejected"
            );
        }
        assert_eq!(
            get_server_url(&pool).await.unwrap(),
            DEFAULT_SERVER_URL,
            "rejected values leave the setting untouched"
        );
    }
}
