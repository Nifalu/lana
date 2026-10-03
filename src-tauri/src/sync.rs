//! Full-snapshot sync (ADR 0002): pull `GET /api/v1/snapshot` from the
//! configured server and replace the SQLite cache wholesale.
//!
//! `sync_now` is the only network seam of the app; everything else reads the
//! local cache, so the app keeps working offline after one successful sync.

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::sqlite::SqlitePool;

use crate::cache;
use crate::snapshot::Snapshot;

/// How long pulling a snapshot may take before giving up. The dataset is a
/// few hundred KB; generous, but bounded so a dead server cannot hang the UI.
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// What a successful sync brought home.
#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub poi_count: usize,
    pub station_count: usize,
    pub generated_at: DateTime<Utc>,
}

/// Builds the one HTTP client every sync reuses: built once per app run
/// (the `AppState` constructor) instead of per sync, with the snapshot
/// fetch timeout baked in.
pub fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .context("failed to build the HTTP client")
}

/// Pulls the full snapshot from `{base_url}/api/v1/snapshot` and replaces the
/// cache in one transaction. On any failure the previous cache content stays
/// untouched.
pub async fn sync_now(
    pool: &SqlitePool,
    http: &reqwest::Client,
    base_url: &str,
) -> anyhow::Result<SyncReport> {
    let snapshot = fetch_snapshot(http, base_url).await?;
    let (poi_count, station_count) = cache::replace_snapshot(pool, &snapshot).await?;
    Ok(SyncReport {
        poi_count,
        station_count,
        generated_at: snapshot.generated_at,
    })
}

/// Fetches and parses the snapshot document from the server.
async fn fetch_snapshot(http: &reqwest::Client, base_url: &str) -> anyhow::Result<Snapshot> {
    let url = format!("{base_url}/api/v1/snapshot");
    let response = http
        .get(&url)
        .send()
        .await
        .with_context(|| format!("failed to reach the server at {url}"))?;
    let response = response
        .error_for_status()
        .with_context(|| format!("server rejected the snapshot request ({url})"))?;
    response
        .json()
        .await
        .with_context(|| format!("server response from {url} is not a valid snapshot"))
}

/// Serializes DB-gated sync tests: they share one per-ticket database
/// (without truncating other tests' rows - seeds are uuid-tagged and cleaned
/// up per test).
#[cfg(test)]
static DB_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

    /// Serves `app` on an ephemeral port (127.0.0.1:0 - never a fixed port,
    /// the port space is shared with other agents) and returns its address.
    async fn spawn(app: axum::Router) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("ephemeral bind works");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("server task failed");
        });
        addr
    }

    /// A migrated Postgres pool against `DATABASE_URL`, or `None` (skip) -
    /// the repo's DB-gated convention.
    async fn pg_pool() -> Option<sqlx::PgPool> {
        match std::env::var("DATABASE_URL") {
            Ok(url) => Some(
                lana_server::db::init_with_url(&url)
                    .await
                    .expect("server db init failed"),
            ),
            Err(_) => {
                eprintln!("DATABASE_URL not set - skipping postgres test");
                None
            }
        }
    }

    /// Seeds one fountain POI and one reporting air station, both tagged
    /// with a unique `source` so parallel tests never collide. Returns the
    /// tag used in the `source` fields (pass it to [`cleanup`]).
    async fn seed_snapshot_data(pool: &sqlx::PgPool) -> uuid::Uuid {
        let tag = uuid::Uuid::new_v4();
        let source = format!("app-sync-test-{tag}");

        sqlx::query(
            "INSERT INTO pois (kind, name, geom, properties, source, source_id) \
             VALUES ('fountain', 'Testbrunnen', \
                     ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography, '{}'::jsonb, $1, 'poi-1')",
        )
        .bind(&source)
        .bind(7.5886_f64)
        .bind(47.5596_f64)
        .execute(pool)
        .await
        .expect("seed poi");

        sqlx::query(
            "INSERT INTO stations (id, kind, name, geom, source) \
             VALUES ($1, 'air', 'Teststation', \
                     ST_SetSRID(ST_MakePoint($3, $4), 4326)::geography, $2)",
        )
        .bind(format!("test-station-{tag}"))
        .bind(&source)
        .bind(7.604586_f64)
        .bind(47.56656_f64)
        .execute(pool)
        .await
        .expect("seed station");

        sqlx::query(
            "INSERT INTO measurements (station_id, measured_at, temperature_c) \
             VALUES ($1, $2, $3)",
        )
        .bind(format!("test-station-{tag}"))
        .bind(Utc::now())
        .bind(30.5_f64)
        .execute(pool)
        .await
        .expect("seed measurement");

        tag
    }

    /// Removes everything this test seeded (never runs on panic, but every
    /// DB-gated server test truncates these tables at start anyway).
    async fn cleanup(pool: &sqlx::PgPool, tag: &uuid::Uuid) {
        let source = format!("app-sync-test-{tag}");
        sqlx::query("DELETE FROM pois WHERE source = $1")
            .bind(&source)
            .execute(pool)
            .await
            .expect("cleanup pois");
        sqlx::query("DELETE FROM stations WHERE source = $1")
            .bind(&source)
            .execute(pool)
            .await
            .expect("cleanup stations (cascades to measurements)");
    }

    /// The primary seam, end to end: sync_now pulls the real server's
    /// snapshot over HTTP (router spawned on an ephemeral port, backed by
    /// the per-ticket Postgres) and the cache serves the data afterwards -
    /// even with the server gone (offline reads).
    #[tokio::test]
    async fn sync_now_pulls_full_snapshot_and_cache_works_offline_afterwards() {
        let Some(pg) = pg_pool().await else {
            return;
        };
        let _db_guard = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tag = seed_snapshot_data(&pg).await;

        let server = spawn(lana_server::api::router(pg.clone())).await;
        let base_url = format!("http://{server}");

        // The temp SQLite cache starts with junk that must not survive.
        let (_dir, pool) = test_pool().await;
        sqlx::query(
            "INSERT INTO pois (kind, name, source, lon, lat, geometry, properties) \
                     VALUES ('fountain', 'STALE', 'old', 0.0, 0.0, '{}', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let report = sync_now(&pool, &http_client().unwrap(), &base_url)
            .await
            .expect("sync_now against the local server");

        // Exactly the rows we seeded are in the snapshot (the shared dev
        // database may carry other tests' rows; ours are uuid-tagged, so we
        // assert ours arrived instead of asserting global counts).
        let pois = crate::cache::list_pois(&pool, None, None).await.unwrap();
        assert!(report.poi_count >= 1);
        assert_eq!(report.poi_count, pois.len(), "report count == stored rows");
        assert!(
            pois.iter()
                .any(|p| p.source.starts_with("app-sync-test-") && p.name == "Testbrunnen"),
            "seeded POI must be in the cache: {pois:?}"
        );
        assert!(
            !pois.iter().any(|p| p.name == "STALE"),
            "wholesale replace: stale row gone"
        );

        let stations = crate::cache::list_stations(&pool, None).await.unwrap();
        assert!(report.station_count >= 1);
        assert_eq!(report.station_count, stations.len());
        let ours = stations
            .iter()
            .find(|s| s.name == "Teststation")
            .expect("seeded station must be in the cache");
        assert_eq!(ours.temperature_c, Some(30.5));
        assert!(ours.measured_at.is_some());

        assert!(
            Utc::now() - report.generated_at < chrono::Duration::minutes(5),
            "generated_at is the server's now"
        );

        let info = crate::cache::cache_info(&pool).await.unwrap();
        let last_sync = info.last_sync_at.expect("last sync time recorded");
        assert!(Utc::now() - last_sync < chrono::Duration::seconds(30));
        assert_eq!(info.generated_at, Some(report.generated_at));

        // The seeded rows must be gone from Postgres for the next run.
        cleanup(&pg, &tag).await;

        // Offline: the server (and its Postgres pool) go away, the cache
        // keeps serving.
        pg.close().await;
        let pois_offline = crate::cache::list_pois(&pool, Some("fountain"), None)
            .await
            .unwrap();
        assert!(pois_offline
            .iter()
            .any(|p| p.source.starts_with("app-sync-test-")));
    }

    /// A failing snapshot pull (server error) leaves the previous cache
    /// content and sync metadata untouched - the app keeps its last good
    /// data. Uses a local 500-router, no database needed.
    #[tokio::test]
    async fn sync_failure_keeps_previous_cache() {
        let broken = axum::Router::new().route(
            "/api/v1/snapshot",
            axum::routing::get(|| async {
                (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "boom")
            }),
        );
        let addr = spawn(broken).await;

        let (_dir, pool) = test_pool().await;
        // A cache with a known previous state.
        sqlx::query(
            "INSERT INTO pois (kind, name, source, lon, lat, geometry, properties) \
                     VALUES ('fountain', 'KEEP', 'old', 7.5, 47.5, '{}', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE cache_meta SET last_sync_at = $1, generated_at = $1 WHERE id = 1")
            .bind(DateTime::parse_from_rfc3339("2026-10-02T10:00:00Z").unwrap())
            .execute(&pool)
            .await
            .unwrap();

        let result = sync_now(&pool, &http_client().unwrap(), &format!("http://{addr}")).await;
        assert!(result.is_err(), "a 500 snapshot must fail the sync");

        let info = cache::cache_info(&pool).await.unwrap();
        assert_eq!(info.poi_count, 1, "previous data kept");
        assert_eq!(
            info.last_sync_at.unwrap().to_rfc3339(),
            "2026-10-02T10:00:00+00:00"
        );
    }
}
