//! Background poller for live measurements (ticket 03).
//!
//! While `serve` runs, a tokio task re-fetches the live datasets roughly
//! every 10 minutes (`LANA_POLL_INTERVAL_SECS`) and upserts the **latest
//! value per station**:
//! - Smart Climate air temperature (dataset 100009), paged newest-first
//!   until consecutive pages stop discovering new stations (bounded by a
//!   page cap); stations join via `name_original` (ticket 02 imported them);
//! - Rhine water temperature (dataset 100046) – one fixed station, the
//!   Rheinüberwachungsstation Weil am Rhein (RUES, "Strang S3"); the dataset
//!   carries no ids or coordinates, so the station row is owned here;
//! - Gartenbäder pool temperatures (dataset 100384), one row per pool per
//!   scraper run, joined on the pool `name`.
//!
//! The poller owns creating/ensuring the station rows for Rhine and pools
//! (kinds `water`/`pool`, sources `ods-100046`/`ods-100384`). Air-station
//! rows embedded in measurement records are ensured too, so `serve` shows
//! live temperatures even on a database that never ran `import`.
//!
//! **Manual trigger:** run one poll cycle on demand with the `poll` CLI mode
//! (`lana-server poll`) or call [`run`] directly from a test with a fixture
//! [`MeasurementSource`] – no waiting for the timer, no network in tests.
//!
//! Split like [`crate::import`]: pure parsing functions are tested against
//! the committed fixtures in `testdata/`; storage goes through idempotent
//! upserts (`ON CONFLICT (station_id, measured_at) DO NOTHING`), so a station
//! that has not reported keeps its last known value and re-polls never
//! duplicate rows.

use anyhow::Context;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::import::StationInsert;

/// Station kinds served by the poller (mirrors the `stations.kind` CHECK;
/// air stations come from the ticket-02 import).
pub const KIND_WATER: &str = "water";
pub const KIND_POOL: &str = "pool";

/// `source` values for the poller-owned datasets.
pub const SOURCE_RHINE: &str = "ods-100046";
pub const SOURCE_POOLS: &str = "ods-100384";

/// The one Rhine station (dataset 100046: Rheinüberwachungsstation Weil am
/// Rhein, sensor strand "Strang S3" – the dataset has no id field).
pub const RHINE_STATION_ID: &str = "rues-s3";

/// The fixed Rhine station row. Position: Swiss LV03 (EPSG:21781)
/// 611740 / 272310 from the dataset's field description, transformed once
/// to WGS84 with PostGIS `ST_Transform` and cross-checked against the
/// swisstopo approximation formulas (see README) – never converted at
/// runtime.
pub const RHINE_STATION_LON: f64 = 7.5947299;
pub const RHINE_STATION_LAT: f64 = 47.6013689;

#[derive(Debug, Clone, PartialEq)]
pub struct MeasurementInsert {
    pub station_id: String,
    pub measured_at: DateTime<Utc>,
    pub temperature_c: Option<f64>,
}

/// One air measurement plus the station row it belongs to (the poller
/// ensures the station so `serve` works on a database that never ran the
/// ticket-02 import).
#[derive(Debug, Clone, PartialEq)]
pub struct AirSample {
    pub station: StationInsert,
    pub measurement: MeasurementInsert,
}

/// One `{total_count, results: [...]}` record page. All three live datasets
/// share this shape; fields are accessed dynamically because only a small
/// subset of each row matters here.
fn record_page(raw: &str, what: &str) -> anyhow::Result<Vec<serde_json::Value>> {
    let value: serde_json::Value =
        serde_json::from_str(raw).with_context(|| format!("{what} page is not valid JSON"))?;
    value
        .get("results")
        .and_then(|r| r.as_array())
        .cloned()
        .with_context(|| format!("{what} page has no results array"))
}

/// Parses the ISO 8601 UTC timestamps used by all datasets
/// (e.g. `"2026-10-02T15:10:02+00:00"`).
fn record_timestamp(record: &serde_json::Value, field: &str) -> anyhow::Result<DateTime<Utc>> {
    let raw = record
        .get(field)
        .and_then(|v| v.as_str())
        .with_context(|| format!("record without {field} timestamp"))?;
    Ok(DateTime::parse_from_rfc3339(raw)?.with_timezone(&Utc))
}

/// Station coordinates: `coords: {lon, lat}` object (same shape as the
/// ticket-02 station dataset; also matches `koordinaten` on 100384).
fn record_lonlat(record: &serde_json::Value, field: &str) -> anyhow::Result<(f64, f64)> {
    let lon = record[field]["lon"]
        .as_f64()
        .with_context(|| format!("record without {field}.lon"))?;
    let lat = record[field]["lat"]
        .as_f64()
        .with_context(|| format!("record without {field}.lat"))?;
    Ok((lon, lat))
}

/// Parses one newest-first air-temperature record page (dataset 100009).
/// `name_original` is the station join key; each row embeds `name_custom`
/// and `coords`, so the poller can ensure station rows without a join.
pub fn parse_air_page(raw: &str) -> anyhow::Result<Vec<AirSample>> {
    record_page(raw, "air-measurements")?
        .into_iter()
        .map(|record| {
            let station_id = record
                .get("name_original")
                .and_then(|v| v.as_str())
                .context("air-measurement record without name_original")?
                .to_string();
            let name = record
                .get("name_custom")
                .and_then(|v| v.as_str())
                .unwrap_or(&station_id)
                .to_string();
            let (lon, lat) = record_lonlat(&record, "coords")
                .with_context(|| format!("air station {station_id:?} without coordinates"))?;
            let measured_at = record_timestamp(&record, "dates_max_date")?;
            let temperature_c = record.get("meta_airtemp").and_then(|v| v.as_f64());
            Ok(AirSample {
                station: StationInsert {
                    id: station_id.clone(),
                    kind: crate::import::KIND_AIR,
                    name,
                    lon,
                    lat,
                    source: crate::import::SOURCE_AIR_STATIONS,
                },
                measurement: MeasurementInsert {
                    station_id,
                    measured_at,
                    temperature_c,
                },
            })
        })
        .collect()
}

/// Parses one newest-first Rhine record page (dataset 100046): every row
/// belongs to the fixed RUES station; the newest row's `rus_w_o_s3_te` is
/// the current water temperature (`startzeitpunkt` its timestamp). Fields
/// may be null when a sensor is offline – tolerated as null values. Returns
/// `None` when the page has no rows.
pub fn parse_rhine(raw: &str) -> anyhow::Result<Option<MeasurementInsert>> {
    let Some(record) = record_page(raw, "rhine")?.into_iter().next() else {
        return Ok(None);
    };
    Ok(Some(MeasurementInsert {
        station_id: RHINE_STATION_ID.to_string(),
        measured_at: record_timestamp(&record, "startzeitpunkt")?,
        temperature_c: record.get("rus_w_o_s3_te").and_then(|v| v.as_f64()),
    }))
}

/// The poller-owned fixed Rhine station row.
pub fn rhine_station() -> StationInsert {
    StationInsert {
        id: RHINE_STATION_ID.to_string(),
        kind: KIND_WATER,
        name: "Rhein – RUES Weil am Rhein".to_string(),
        lon: RHINE_STATION_LON,
        lat: RHINE_STATION_LAT,
        source: SOURCE_RHINE,
    }
}

/// One pool measurement plus its station row (id: slugified pool name).
#[derive(Debug, Clone, PartialEq)]
pub struct PoolSample {
    pub station: StationInsert,
    pub measurement: MeasurementInsert,
}

/// Raw record pages for the live datasets. The production implementation
/// ([`crate::ods::OdsClient`]) fetches from data.bs.ch; tests feed scripted
/// or fixture pages, keeping the poller offline.
pub(crate) trait MeasurementSource {
    /// One newest-first page of air-temperature records (dataset 100009),
    /// starting at `offset` rows. The source chooses its page size; pages
    /// arrive sorted by measurement timestamp, newest first.
    async fn air_page(&self, offset: usize) -> anyhow::Result<String>;
    /// One newest-first page of Rhine water values (dataset 100046).
    async fn rhine(&self) -> anyhow::Result<String>;
    /// One newest-first page of pool temperatures (dataset 100384).
    async fn pools(&self) -> anyhow::Result<String>;
}

/// Air paging bounds. The walk is newest-first over a 6.8M-row dataset: it
/// stops once [`AIR_STALL_PAGE_LIMIT`] consecutive pages discover no new
/// station (an empty/short page also stalls out), and never fetches more
/// than [`AIR_PAGE_CAP`] pages of [`AIR_PAGE_SIZE`] rows regardless.
pub const AIR_PAGE_SIZE: usize = 100;
pub const AIR_PAGE_CAP: usize = 20;
pub const AIR_STALL_PAGE_LIMIT: usize = 2;

/// Walks the air-measurement dataset newest-first and returns the **latest
/// value per station**: the first occurrence of each station wins (pages
/// are newest-first, so that is its newest row). Stops after
/// [`AIR_STALL_PAGE_LIMIT`] consecutive pages without a new station, or at
/// [`AIR_PAGE_CAP`] pages.
pub(crate) async fn fetch_latest_air(
    source: &impl MeasurementSource,
) -> anyhow::Result<Vec<AirSample>> {
    let mut latest: Vec<AirSample> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut offset = 0;
    let mut stall = 0;
    for _ in 0..AIR_PAGE_CAP {
        let page = source.air_page(offset).await?;
        let samples = parse_air_page(&page)?;
        offset += samples.len();
        let new_stations = samples
            .iter()
            .filter(|s| seen.insert(s.measurement.station_id.clone()))
            .count();
        // Pages are newest-first, so the first occurrence of a station is
        // its newest row within the walk.
        if new_stations > 0 {
            let fresh: Vec<AirSample> = samples
                .into_iter()
                .filter(|s| {
                    !latest
                        .iter()
                        .any(|l| l.measurement.station_id == s.measurement.station_id)
                })
                .collect();
            latest.extend(fresh);
        }
        stall = if new_stations == 0 { stall + 1 } else { 0 };
        if stall >= AIR_STALL_PAGE_LIMIT {
            break;
        }
    }
    Ok(latest)
}

/// What one poll cycle loaded; the serve loop and the `poll` mode log it.
/// Its `Display` is the shared "poll complete" log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollSummary {
    /// Distinct air stations refreshed from dataset 100009.
    pub air_stations: usize,
    /// Whether a Rhine water value was fetched.
    pub rhine_updated: bool,
    /// Pools refreshed from dataset 100384.
    pub pools: usize,
}

impl std::fmt::Display for PollSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "poll complete: {} air stations, Rhine water {}, {} pools refreshed",
            self.air_stations,
            if self.rhine_updated { "yes" } else { "no" },
            self.pools
        )
    }
}

/// Upserts measurements keyed on `(station_id, measured_at)`: re-polling
/// the same values conflicts and is ignored (idempotent), a station that
/// has not reported keeps its last known row, and newer readings insert a
/// new row (history is kept; the snapshot serves the newest).
pub async fn upsert_measurements(pool: &PgPool, rows: &[MeasurementInsert]) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for row in rows {
        sqlx::query(
            "INSERT INTO measurements (station_id, measured_at, temperature_c) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (station_id, measured_at) DO NOTHING",
        )
        .bind(&row.station_id)
        .bind(row.measured_at)
        .bind(row.temperature_c)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// One manual poll cycle – the trigger the serve loop, the `poll` CLI mode
/// and the tests call. Fetches all three live datasets from `source`,
/// ensures the station rows (Rhine/pool stations are owned here; air
/// stations are re-ensured from the measurement rows so `serve` works
/// without a prior ticket-02 import) and upserts the latest values.
pub(crate) async fn run(
    pool: &PgPool,
    source: &impl MeasurementSource,
) -> anyhow::Result<PollSummary> {
    // Rhine water temperature: one fixed station owned by the poller.
    let rhine = parse_rhine(&source.rhine().await?)?;
    if rhine.is_some() {
        crate::import::upsert_stations(pool, &[rhine_station()]).await?;
        upsert_measurements(pool, rhine.as_slice()).await?;
    }

    // Pool temperatures: stations derive from the pool names.
    let pools = parse_pools(&source.pools().await?)?;
    let pool_count = pools.len();
    if !pools.is_empty() {
        let stations: Vec<StationInsert> = pools.iter().map(|p| p.station.clone()).collect();
        crate::import::upsert_stations(pool, &stations).await?;
        let rows: Vec<MeasurementInsert> = pools.into_iter().map(|p| p.measurement).collect();
        upsert_measurements(pool, &rows).await?;
    }

    // Air temperatures: page newest-first, latest value per station.
    let air = fetch_latest_air(source).await?;
    let air_count = air.len();
    if !air.is_empty() {
        let stations: Vec<StationInsert> = air.iter().map(|s| s.station.clone()).collect();
        crate::import::upsert_stations(pool, &stations).await?;
        let rows: Vec<MeasurementInsert> = air.into_iter().map(|s| s.measurement).collect();
        upsert_measurements(pool, &rows).await?;
    }

    Ok(PollSummary {
        air_stations: air_count,
        rhine_updated: rhine.is_some(),
        pools: pool_count,
    })
}

/// The serve-mode poll loop: one poll immediately, then every `interval`.
/// Failures are logged and retried on the next tick – they never take the
/// API down. Runs until the task is aborted (server shutdown).
pub(crate) async fn run_loop(
    pool: PgPool,
    source: impl MeasurementSource,
    interval: std::time::Duration,
) {
    loop {
        match run(&pool, &source).await {
            Ok(summary) => println!("{summary}"),
            Err(err) => eprintln!("poll failed: {err:#} – retrying next tick"),
        }
        tokio::time::sleep(interval).await;
    }
}

/// Parses one newest-first pool-temperature record page (dataset 100384).
/// Rows are snapshotted per scraper run and repeat per pool name, so rows
/// are deduplicated by `name`, keeping the newest `zeitpunkt_job`. The pool
/// name is the only identity the dataset offers, so the station id is its
/// deterministic slug ([`slugify`]).
pub fn parse_pools(raw: &str) -> anyhow::Result<Vec<PoolSample>> {
    let mut by_name: Vec<(String, PoolSample)> = Vec::new();
    for record in record_page(raw, "pools")? {
        let name = record
            .get("name")
            .and_then(|v| v.as_str())
            .context("pool record without name")?
            .to_string();
        let (lon, lat) = record_lonlat(&record, "koordinaten")
            .with_context(|| format!("pool {name:?} without coordinates"))?;
        let measured_at = record_timestamp(&record, "zeitpunkt_job")?;
        let temperature_c = record.get("temperatur").and_then(|v| v.as_f64());
        let station_id = slugify(&name);
        let sample = PoolSample {
            station: StationInsert {
                id: station_id.clone(),
                kind: KIND_POOL,
                name: name.clone(),
                lon,
                lat,
                source: SOURCE_POOLS,
            },
            measurement: MeasurementInsert {
                station_id,
                measured_at,
                temperature_c,
            },
        };
        match by_name.iter_mut().find(|(existing, _)| *existing == name) {
            Some((_, kept)) => {
                if sample.measurement.measured_at > kept.measurement.measured_at {
                    *kept = sample;
                }
            }
            None => by_name.push((name, sample)),
        }
    }
    Ok(by_name.into_iter().map(|(_, sample)| sample).collect())
}

/// Deterministic station id from a pool name: lowercased, German umlauts
/// transliterated (ä→ae, ö→oe, ü→ue, ß→ss), runs of non-alphanumeric
/// characters collapsed to one dash, dashes at the ends trimmed. Names come
/// from the dataset (presumed stable); a renamed pool would appear as a new
/// station, leaving the old one with its last known value.
pub fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut last_was_dash = true; // suppresses a leading dash
    for ch in name.chars() {
        for mapped in transliterate(ch) {
            if mapped.is_ascii_alphanumeric() {
                slug.push(mapped);
                last_was_dash = false;
            } else if !last_was_dash {
                slug.push('-');
                last_was_dash = true;
            }
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

fn transliterate(ch: char) -> Vec<char> {
    match ch {
        'ä' | 'Ä' => vec!['a', 'e'],
        'ö' | 'Ö' => vec!['o', 'e'],
        'ü' | 'Ü' => vec!['u', 'e'],
        'ß' => vec!['s', 's'],
        c if c.is_ascii_alphanumeric() => vec![c.to_ascii_lowercase()],
        _ => vec!['-'],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const AIR_MEASUREMENTS_FIXTURE: &str = include_str!("../testdata/air-measurements.json");
    const RHINE_FIXTURE: &str = include_str!("../testdata/rhine.json");
    const POOLS_FIXTURE: &str = include_str!("../testdata/pools.json");

    /// An air-measurement page parses into per-station samples keyed by
    /// `name_original`, with the measurement timestamp from `dates_max_date`
    /// and the value from `meta_airtemp` (fixture: 5 newest-first rows).
    #[test]
    fn parses_air_measurement_page_fixture() {
        let samples = parse_air_page(AIR_MEASUREMENTS_FIXTURE).unwrap();
        assert_eq!(samples.len(), 5);

        let first = &samples[0];
        assert_eq!(first.station.id, "03409FF2");
        assert_eq!(first.station.name, "Güterstrasse");
        assert_eq!(first.station.kind, crate::import::KIND_AIR);
        assert_eq!(first.station.source, crate::import::SOURCE_AIR_STATIONS);
        assert_eq!(first.station.lon, 7.58816);
        assert_eq!(first.station.lat, 47.54538);

        assert_eq!(first.measurement.station_id, "03409FF2");
        assert_eq!(
            first.measurement.measured_at,
            chrono::Utc
                .with_ymd_and_hms(2026, 10, 2, 15, 10, 2)
                .unwrap()
        );
        assert_eq!(first.measurement.temperature_c, Some(21.06));

        // Newest-first ordering: the fixture's rows arrive newest first.
        assert!(
            samples[0].measurement.measured_at > samples[4].measurement.measured_at,
            "fixture page must be newest-first"
        );
    }

    /// A missing temperature (sensor offline) parses to `None`, not an error.
    #[test]
    fn air_sample_without_temperature_parses_to_none() {
        let raw = serde_json::json!({
            "total_count": 1,
            "results": [{
                "name_original": "TEST0001",
                "name_custom": "Teststation",
                "dates_max_date": "2026-10-02T15:10:02+00:00",
                "meta_airtemp": null,
                "coords": {"lon": 7.5, "lat": 47.5}
            }]
        });
        let samples = parse_air_page(&raw.to_string()).unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].measurement.temperature_c, None);
    }

    /// The Rhine dataset (100046) has no station id field: every row belongs
    /// to the fixed RUES Weil am Rhein station, whose position only exists as
    /// LV03 coordinates in a field description. Parsing yields one measurement
    /// for that station (newest row wins; `startzeitpunkt` is the timestamp,
    /// `rus_w_o_s3_te` the temperature).
    #[test]
    fn parses_rhine_page_onto_the_fixed_rues_station() {
        let measurement = parse_rhine(RHINE_FIXTURE).unwrap().expect("newest row");
        assert_eq!(measurement.station_id, RHINE_STATION_ID);
        assert_eq!(
            measurement.measured_at,
            chrono::Utc
                .with_ymd_and_hms(2026, 10, 2, 14, 45, 0)
                .unwrap()
        );
        assert_eq!(measurement.temperature_c, Some(20.72));

        // The fixed station row: water kind, its own source, position
        // transformed once from LV03 611740/272310 (EPSG:21781) to WGS84
        // via PostGIS ST_Transform – verified against the swisstopo
        // approximation formulas (see README).
        let station = rhine_station();
        assert_eq!(station.id, "rues-s3");
        assert_eq!(station.kind, KIND_WATER);
        assert_eq!(station.source, SOURCE_RHINE);
        assert_eq!(station.name, "Rhein – RUES Weil am Rhein");
        assert!((station.lon - 7.5947299).abs() < 1e-6);
        assert!((station.lat - 47.6013689).abs() < 1e-6);
    }

    /// A newest Rhine row without a temperature (sensor offline) still
    /// records the timestamp with a null value.
    #[test]
    fn rhine_row_without_temperature_records_null_value() {
        let raw = serde_json::json!({
            "total_count": 1,
            "results": [{
                "startzeitpunkt": "2026-10-02T15:00:00+00:00",
                "rus_w_o_s3_te": null
            }]
        });
        let measurement = parse_rhine(&raw.to_string()).unwrap().unwrap();
        assert_eq!(measurement.temperature_c, None);
    }

    /// An empty Rhine page (dataset unreachable row / no rows yet) yields no
    /// measurement instead of an error.
    #[test]
    fn rhine_page_without_rows_yields_no_measurement() {
        let raw = serde_json::json!({"total_count": 0, "results": []});
        assert_eq!(parse_rhine(&raw.to_string()).unwrap(), None);
    }

    /// Pool temperatures (dataset 100384) parse into pool stations keyed by
    /// the slugified pool `name` (the dataset has no id), with coordinates
    /// from `koordinaten` and the scraper-run time as timestamp.
    #[test]
    fn parses_pools_page_fixture() {
        let samples = parse_pools(POOLS_FIXTURE).unwrap();
        assert_eq!(samples.len(), 5);

        let eglisee = samples
            .iter()
            .find(|s| s.station.name == "Hallenbad Eglisee")
            .unwrap();
        assert_eq!(eglisee.station.id, "hallenbad-eglisee");
        assert_eq!(eglisee.station.kind, KIND_POOL);
        assert_eq!(eglisee.station.source, SOURCE_POOLS);
        assert_eq!(eglisee.station.lon, 7.61478);
        assert_eq!(eglisee.station.lat, 47.570491);
        assert_eq!(eglisee.measurement.station_id, "hallenbad-eglisee");
        assert_eq!(eglisee.measurement.temperature_c, Some(21.0));
        assert_eq!(
            eglisee.measurement.measured_at,
            chrono::Utc
                .with_ymd_and_hms(2026, 10, 2, 15, 0, 18)
                .unwrap()
        );
    }

    /// Rows are snapshotted per scraper run, so one page can straddle two
    /// runs: a pool repeated with a newer `zeitpunkt_job` keeps only the
    /// newest sample.
    #[test]
    fn pools_page_dedupes_by_name_keeping_the_newest_run() {
        let raw = serde_json::json!({
            "total_count": 2,
            "results": [
                {"name": "Bachgraben Sportbad", "temperatur": 20,
                 "zeitpunkt_job": "2026-10-02T14:00:18+00:00",
                 "koordinaten": {"lon": 7.556763, "lat": 47.56169}},
                {"name": "Bachgraben Sportbad", "temperatur": 21,
                 "zeitpunkt_job": "2026-10-02T15:00:18+00:00",
                 "koordinaten": {"lon": 7.556763, "lat": 47.56169}}
            ]
        });
        let samples = parse_pools(&raw.to_string()).unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].measurement.temperature_c, Some(21.0));
        assert_eq!(
            samples[0].measurement.measured_at,
            chrono::Utc
                .with_ymd_and_hms(2026, 10, 2, 15, 0, 18)
                .unwrap()
        );
    }

    /// Station ids derive deterministically from the pool name: lowercased,
    /// umlauts transliterated, everything non-alphanumeric collapsed to one
    /// dash (trimmed at the ends).
    #[test]
    fn pool_names_slugify_deterministically() {
        assert_eq!(slugify("Hallenbad Eglisee"), "hallenbad-eglisee");
        assert_eq!(slugify("Bachgraben Sportbad"), "bachgraben-sportbad");
        assert_eq!(
            slugify("Schwimmbad Schützenmatt"),
            "schwimmbad-schuetzenmatt"
        );
        assert_eq!(slugify("  Bad   beim Ring! "), "bad-beim-ring");
    }

    // -- Air paging --------------------------------------------------------

    /// Builds one air-measurement record for a synthetic station (`minutes`
    /// after 00:00 UTC on the fixture date).
    fn air_row(station: &str, minutes: u32, temp: f64) -> serde_json::Value {
        serde_json::json!({
            "name_original": station,
            "name_custom": station,
            "dates_max_date": format!(
                "2026-10-{:02}T{:02}:{:02}:{:02}+00:00",
                2 + minutes / (24 * 60),
                minutes / 60 % 24,
                minutes % 60,
                0
            ),
            "meta_airtemp": temp,
            "coords": {"lon": 7.5, "lat": 47.5}
        })
    }

    fn page_of(rows: Vec<serde_json::Value>) -> String {
        serde_json::json!({"total_count": 6_824_533, "results": rows}).to_string()
    }

    /// A fake measurement source that hands out scripted pages and counts
    /// how many air pages were fetched.
    struct ScriptedAir {
        pages: Vec<String>,
        fetched: std::cell::Cell<usize>,
    }

    impl MeasurementSource for ScriptedAir {
        async fn air_page(&self, offset: usize) -> anyhow::Result<String> {
            let index = offset / 5; // scripted pages are 5 rows wide
            self.fetched.set(self.fetched.get() + 1);
            Ok(self
                .pages
                .get(index)
                .cloned()
                .unwrap_or_else(|| page_of(Vec::new())))
        }
        async fn rhine(&self) -> anyhow::Result<String> {
            Ok(RHINE_FIXTURE.to_string())
        }
        async fn pools(&self) -> anyhow::Result<String> {
            Ok(POOLS_FIXTURE.to_string())
        }
    }

    /// Paging stops once consecutive pages stop discovering new stations:
    /// page 1 re-offers the five stations of page 0 (stall 1), page 2 has a
    /// new station F (stall reset), pages 3 and 4 have none (stall 2 →
    /// stop). Only the newest value per station is kept; page 5 is never
    /// fetched.
    #[tokio::test]
    async fn air_paging_stops_after_consecutive_pages_without_new_stations() {
        let source = ScriptedAir {
            pages: vec![
                // page 0: stations A..E, newest values
                page_of(vec![
                    air_row("A", 0, 21.0),
                    air_row("B", 10, 22.0),
                    air_row("C", 20, 23.0),
                    air_row("D", 30, 24.0),
                    air_row("E", 40, 25.0),
                ]),
                // page 1: no new stations (stall 1)
                page_of(vec![
                    air_row("A", 60, 20.0),
                    air_row("B", 70, 21.0),
                    air_row("C", 80, 22.0),
                    air_row("D", 90, 23.0),
                    air_row("E", 100, 24.0),
                ]),
                // page 2: one new station F (stall resets)
                page_of(vec![
                    air_row("F", 50, 26.0),
                    air_row("A", 120, 19.0),
                    air_row("B", 130, 20.0),
                    air_row("C", 140, 21.0),
                    air_row("D", 150, 22.0),
                ]),
                // page 3: no new stations (stall 1)
                page_of(vec![
                    air_row("F", 110, 25.0),
                    air_row("E", 160, 23.0),
                    air_row("A", 180, 18.0),
                    air_row("B", 190, 19.0),
                    air_row("C", 200, 20.0),
                ]),
                // page 4: still no new stations (stall 2 → stop)
                page_of(vec![
                    air_row("D", 210, 21.0),
                    air_row("E", 220, 22.0),
                    air_row("F", 170, 24.0),
                    air_row("A", 240, 17.0),
                    air_row("B", 250, 18.0),
                ]),
                // page 5 would offer G – must never be fetched
                page_of(vec![air_row("G", 0, 30.0)]),
            ],
            fetched: std::cell::Cell::new(0),
        };

        let latest = fetch_latest_air(&source).await.unwrap();
        assert_eq!(source.fetched.get(), 5, "stops before the sixth page");

        let mut ids: Vec<&str> = latest
            .iter()
            .map(|s| s.measurement.station_id.as_str())
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["A", "B", "C", "D", "E", "F"]);

        // The kept value per station is its newest (first occurrence in
        // newest-first order), not an older one from a later page.
        let a = latest
            .iter()
            .find(|s| s.measurement.station_id == "A")
            .unwrap();
        assert_eq!(a.measurement.temperature_c, Some(21.0));
        let f = latest
            .iter()
            .find(|s| s.measurement.station_id == "F")
            .unwrap();
        assert_eq!(f.measurement.temperature_c, Some(26.0));
    }

    /// The page cap bounds the walk even when every page keeps discovering
    /// new stations (the fake never repeats a station).
    #[tokio::test]
    async fn air_paging_is_bounded_by_the_page_cap() {
        let mut pages = Vec::new();
        for page in 0..40 {
            let mut rows = Vec::new();
            for row in 0..5 {
                let station = format!("S{:03}", page * 5 + row);
                rows.push(air_row(&station, 0, 20.0));
            }
            pages.push(page_of(rows));
        }
        let source = ScriptedAir {
            pages,
            fetched: std::cell::Cell::new(0),
        };

        let latest = fetch_latest_air(&source).await.unwrap();
        assert_eq!(source.fetched.get(), AIR_PAGE_CAP, "stops at the page cap");
        assert_eq!(latest.len(), AIR_PAGE_CAP * 5);
    }

    // -- Storage / manual trigger (DB-gated) -------------------------------

    /// One manual poll cycle (`run` is the trigger tests and the `poll` CLI
    /// mode call): inserts the latest value per station, creates the Rhine
    /// and pool stations it owns, and is idempotent – a second identical
    /// poll changes nothing (conflicts on station+timestamp are ignored).
    #[tokio::test]
    async fn manual_poll_inserts_measurements_and_is_idempotent() {
        let Some(pool) = crate::test_support::db_pool().await else {
            return;
        };
        let _db = crate::test_support::lock_db().await;
        crate::test_support::reset_db(&pool).await;

        // Ticket-02 import fills the air stations (10 in the fixture);
        // Rhine and pool stations do not exist yet – the poller owns them.
        crate::import::run(&pool, &crate::test_support::FixtureSource::default())
            .await
            .unwrap();

        let summary = run(&pool, &crate::test_support::FixtureSource::default())
            .await
            .unwrap();
        assert_eq!(summary.air_stations, 5);
        assert!(summary.rhine_updated);
        assert_eq!(summary.pools, 5);

        let measurement_count = || async {
            let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM measurements")
                .fetch_one(&pool)
                .await
                .unwrap();
            count
        };
        // 5 air + 1 rhine + 5 pools, one row each.
        assert_eq!(measurement_count().await, 11);

        // A second identical poll conflicts on (station, measured_at) and
        // must insert nothing new.
        run(&pool, &crate::test_support::FixtureSource::default())
            .await
            .unwrap();
        assert_eq!(measurement_count().await, 11, "re-poll must not duplicate");

        // The poller created its stations: the fixed Rhine station (water)
        // and the pools (pool kind, slugified ids).
        let (rhine_kind, rhine_source): (String, String) =
            sqlx::query_as("SELECT kind, source FROM stations WHERE id = 'rues-s3'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(rhine_kind, KIND_WATER);
        assert_eq!(rhine_source, SOURCE_RHINE);
        let (pool_kind, pool_name): (String, String) =
            sqlx::query_as("SELECT kind, name FROM stations WHERE id = 'hallenbad-eglisee'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(pool_kind, KIND_POOL);
        assert_eq!(pool_name, "Hallenbad Eglisee");

        pool.close().await;
    }

    /// A station that has not reported again keeps its last known value:
    /// later polls with an empty air feed change nothing, and a newer
    /// reading replaces the served latest value without deleting history.
    #[tokio::test]
    async fn silent_stations_keep_their_last_known_value() {
        let Some(pool) = crate::test_support::db_pool().await else {
            return;
        };
        let _db = crate::test_support::lock_db().await;
        crate::test_support::reset_db(&pool).await;

        run(&pool, &crate::test_support::FixtureSource::default())
            .await
            .unwrap();

        // The station stops reporting: an empty air feed inserts nothing and
        // must not delete the previous value.
        let silent = crate::test_support::FixtureSource {
            air_measurements: r#"{"total_count": 0, "results": []}"#.to_string(),
            ..Default::default()
        };
        run(&pool, &silent).await.unwrap();
        let (kept,): (Option<f64>,) =
            sqlx::query_as("SELECT temperature_c FROM measurements WHERE station_id = '03409FF2'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(kept, Some(21.06), "last known value survives");

        // A newer reading arrives (16:00, after the fixture's 15:10): it
        // becomes the latest value (a new row; the old one stays as history).
        let mut fresher = crate::test_support::FixtureSource {
            air_measurements: page_of(vec![air_row("03409FF2", 16 * 60, 22.5)]),
            ..Default::default()
        };
        fresher.pools = r#"{"total_count": 0, "results": []}"#.to_string();
        fresher.rhine = r#"{"total_count": 0, "results": []}"#.to_string();
        let summary = run(&pool, &fresher).await.unwrap();
        assert_eq!(summary.air_stations, 1);
        let (latest,): (Option<f64>,) = sqlx::query_as(
            "SELECT temperature_c FROM measurements \n             WHERE station_id = '03409FF2' \n             ORDER BY measured_at DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(latest, Some(22.5));
        let (history,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM measurements WHERE station_id = '03409FF2'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(history, 2, "the older reading stays as history");

        pool.close().await;
    }
}
