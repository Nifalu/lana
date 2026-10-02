//! PostgreSQL setup and migrations.
//!
//! The connection string comes from `DATABASE_URL` (the nix dev shell exports
//! a default pointing at the local cluster created by `just db-init`).
//! Schema changes are plain SQL files in `migrations/`, applied in filename
//! order at app startup by sqlx. Never edit an already-applied migration –
//! add a new numbered file instead.

use anyhow::Context;
use sqlx::PgPool;

/// Connects to Postgres and applies pending migrations.
pub async fn init() -> anyhow::Result<PgPool> {
    let url = std::env::var("DATABASE_URL").context(
        "DATABASE_URL is not set – the nix dev shell exports a default; \
         create the local cluster with `just db-init && just db-start && just db-createdb`",
    )?;
    let pool = PgPool::connect(&url)
        .await
        .context("failed to connect to Postgres (is the server running? try `just db-start`)")?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("failed to apply migrations")?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requires a running Postgres and `DATABASE_URL` (both provided by the
    /// nix dev shell + `just db-start`). Skips silently otherwise.
    #[test]
    fn migrations_apply() {
        if std::env::var("DATABASE_URL").is_err() {
            eprintln!("DATABASE_URL not set – skipping postgres test");
            return;
        }
        tauri::async_runtime::block_on(async move {
            let pool = init().await.expect("db::init failed");
            let (one,): (i32,) = sqlx::query_as("SELECT 1")
                .fetch_one(&pool)
                .await
                .expect("query failed");
            assert_eq!(one, 1);
            pool.close().await;
        });
    }
}
