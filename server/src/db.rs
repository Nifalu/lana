//! PostgreSQL setup and migrations.
//!
//! The connection string comes from `DATABASE_URL` (the nix dev shell exports
//! a default pointing at the local dev cluster). Schema changes are plain SQL
//! files in `migrations/`, applied in filename order at server startup.
//! Never edit an already-applied migration – add a new numbered file instead.
//!
//! The server keeps its own migration bookkeeping table
//! (`_sqlx_server_migrations`) so the Tauri app and the server can share one
//! dev database without fighting over `_sqlx_migrations`.

use anyhow::Context;
use sqlx::PgPool;

/// Connects to Postgres (via `DATABASE_URL`) and applies pending migrations.
pub async fn init() -> anyhow::Result<PgPool> {
    let url = std::env::var("DATABASE_URL").context(
        "DATABASE_URL is not set – the nix dev shell exports a default; \
         create the local cluster with `just db-init && just db-start && just db-createdb`",
    )?;
    init_with_url(&url).await
}

/// Connects to Postgres at `url` and applies pending migrations.
pub async fn init_with_url(url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPool::connect(url)
        .await
        .context("failed to connect to Postgres (is the server running? try `just db-start`)")?;
    run_migrations(&pool)
        .await
        .context("failed to apply migrations")?;
    Ok(pool)
}

async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    let mut migrator = sqlx::migrate!("./migrations");
    // Not dangerous for us: the server owns its migrations and the separate
    // table lets it coexist with the Tauri app on one dev database.
    migrator.dangerous_set_table_name("_sqlx_server_migrations");
    migrator.run(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requires a running Postgres and `DATABASE_URL` (both provided by the
    /// nix dev shell + `just db-start`). Skips silently otherwise.
    #[tokio::test]
    async fn migrations_bootstrap_postgis_and_create_schema() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("DATABASE_URL not set – skipping postgres test");
            return;
        };
        let pool = init_with_url(&url).await.expect("db init failed");

        // PostGIS is bootstrapped by the first migration.
        let (ext,): (String,) =
            sqlx::query_as("SELECT extname FROM pg_extension WHERE extname = 'postgis'")
                .fetch_one(&pool)
                .await
                .expect("postgis extension should be installed");
        assert_eq!(ext, "postgis");

        // The core schema tables exist.
        for table in [
            "pois",
            "stations",
            "measurements",
            "devices",
            "help_requests",
            "help_request_notified",
        ] {
            let (name,): (Option<String>,) = sqlx::query_as("SELECT to_regclass($1)::text")
                .bind(table)
                .fetch_one(&pool)
                .await
                .expect("to_regclass query failed");
            assert_eq!(name.as_deref(), Some(table), "table {table} should exist");
        }

        // POI positions are stored as geography (PostGIS, lon/lat SRID 4326);
        // the type is GEOMETRY (not POINT) so swim-area polygons fit.
        let (srid,): (i32,) = sqlx::query_as(
            "SELECT srid FROM geography_columns \
             WHERE f_table_name = 'pois' AND f_geography_column = 'geom'",
        )
        .fetch_one(&pool)
        .await
        .expect("pois.geom should be a geography column");
        assert_eq!(srid, 4326);
        let (geom_type,): (String,) = sqlx::query_as(
            "SELECT type FROM geography_columns \
             WHERE f_table_name = 'pois' AND f_geography_column = 'geom'",
        )
        .fetch_one(&pool)
        .await
        .expect("pois.geom should be a geography column");
        assert_eq!(geom_type, "Geometry");

        // Stations expose their origin dataset via the source column.
        let (source_col,): (Option<String>,) = sqlx::query_as(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_name = 'stations' AND column_name = 'source'",
        )
        .fetch_one(&pool)
        .await
        .expect("information_schema query failed");
        assert_eq!(source_col.as_deref(), Some("source"));

        // Device positions use the same geography convention.
        let (srid,): (i32,) = sqlx::query_as(
            "SELECT srid FROM geography_columns \
             WHERE f_table_name = 'devices' AND f_geography_column = 'last_location'",
        )
        .fetch_one(&pool)
        .await
        .expect("devices.last_location should be a geography column");
        assert_eq!(srid, 4326);

        // Migration 0005 dropped the helper availability windows.
        let (windows,): (Option<String>,) =
            sqlx::query_as("SELECT to_regclass('helper_windows')::text")
                .fetch_one(&pool)
                .await
                .expect("to_regclass query failed");
        assert_eq!(windows, None, "helper_windows must be gone");

        pool.close().await;
    }
}
