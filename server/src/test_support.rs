//! Shared helpers for DB-gated tests (compiled only under `cfg(test)`).
//!
//! Convention: DB-gated tests skip silently when `DATABASE_URL` is unset.
//! They share one per-ticket database, so a global lock serializes them; each
//! test truncates the tables it needs via [`reset_db`].

use std::sync::Mutex;
use std::sync::MutexGuard;

use sqlx::PgPool;

use crate::db;
use crate::ods::DatasetSource;

static DB_LOCK: Mutex<()> = Mutex::new(());

/// Serializes DB-gated tests (they reset one shared database).
pub async fn lock_db() -> MutexGuard<'static, ()> {
    match DB_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// A migrated test pool, or `None` (skip) when `DATABASE_URL` is unset.
pub async fn db_pool() -> Option<PgPool> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("DATABASE_URL not set - skipping postgres test");
            return None;
        }
    };
    Some(db::init_with_url(&url).await.expect("db init failed"))
}

/// Empties the POI/station tables so a test starts from a known state.
pub async fn reset_db(pool: &PgPool) {
    sqlx::query("TRUNCATE pois, stations, measurements RESTART IDENTITY CASCADE")
        .execute(pool)
        .await
        .expect("truncate failed");
}

/// DatasetSource backed by the committed fixture files in `testdata/` - the
/// same bodies the real Opendatasoft client would deliver, minus the network.
/// Implements both the import source and the poller's [`MeasurementSource`],
/// so tests drive one fixture set end to end. Individual fields are public
/// and overridable (e.g. an empty air feed to simulate a silent station).
pub(crate) struct FixtureSource {
    pub fountains: String,
    pub air_measurements: String,
    pub rhine: String,
    pub pools: String,
}

impl Default for FixtureSource {
    fn default() -> Self {
        Self {
            fountains: include_str!("../testdata/fountains.geojson").to_string(),
            air_measurements: include_str!("../testdata/air-measurements.json").to_string(),
            rhine: include_str!("../testdata/rhine.json").to_string(),
            pools: include_str!("../testdata/pools.json").to_string(),
        }
    }
}

impl DatasetSource for FixtureSource {
    async fn fountains(&self) -> anyhow::Result<String> {
        Ok(self.fountains.clone())
    }

    async fn swim_areas(&self) -> anyhow::Result<String> {
        Ok(include_str!("../testdata/swim-areas.geojson").to_string())
    }

    async fn air_stations(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![
            include_str!("../testdata/air-stations.json").to_string(),
            include_str!("../testdata/air-stations-page2.json").to_string(),
        ])
    }
}

impl crate::poller::MeasurementSource for FixtureSource {
    async fn air_page(&self, offset: usize) -> anyhow::Result<String> {
        // The fixture is one 5-row page; anything beyond it is exhausted.
        if offset == 0 {
            Ok(self.air_measurements.clone())
        } else {
            Ok(r#"{"total_count": 5, "results": []}"#.to_string())
        }
    }

    async fn rhine(&self) -> anyhow::Result<String> {
        Ok(self.rhine.clone())
    }

    async fn pools(&self) -> anyhow::Result<String> {
        Ok(self.pools.clone())
    }
}
