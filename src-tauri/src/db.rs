//! Embedded SQLite cache setup (ADR 0002).
//!
//! The app is a client of the lana server and owns no Postgres: it embeds a
//! SQLite database in the platform app-data directory as its offline cache.
//! Schema changes are plain SQL files in `migrations/`, applied in filename
//! order at startup by sqlx. Never edit an already-applied migration - add a
//! new numbered file instead.

use std::path::Path;

use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};

/// File name of the cache database inside the app-data directory.
const CACHE_FILE: &str = "cache.sqlite3";

/// Opens (creating if needed) the cache database in `app_data_dir` and
/// applies pending migrations.
pub async fn init(app_data_dir: &Path) -> anyhow::Result<SqlitePool> {
    open_at(&app_data_dir.join(CACHE_FILE)).await
}

/// Opens (creating if needed) the cache database at `path` and applies
/// pending migrations. Tests use this to work on a temp file.
pub async fn open_at(path: &Path) -> anyhow::Result<SqlitePool> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        // WAL lets map reads proceed while a sync writes (mobile-friendly).
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .with_context(|| format!("failed to open SQLite cache at {}", path.display()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to apply cache migrations")?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh cache file is created with the full schema applied (the
    /// settings/meta tables exist and are queryable).
    #[tokio::test]
    async fn opens_new_cache_and_applies_migrations() {
        let dir = tempfile::tempdir().unwrap();
        let pool = open_at(&dir.path().join("subdir/cache.sqlite3"))
            .await
            .expect("open_at failed");
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM cache_meta")
            .fetch_one(&pool)
            .await
            .expect("cache_meta should exist");
        assert_eq!(count, 1, "exactly one cache_meta row");
        pool.close().await;
    }
}
