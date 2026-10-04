//! Data access for the on-device SQLite cache.
//!
//! Everything the Tauri commands expose lives here as plain async functions
//! over a [`SqlitePool`], so the seams are testable without a Tauri app:
//! device identity + server URL (the `settings` table), POI/station queries,
//! wholesale snapshot replacement and cache metadata.

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::sqlite::SqlitePool;

use crate::snapshot;

/// Settings-table key holding the persistent device id (UUID v4).
const DEVICE_ID_KEY: &str = "device_id";

/// Settings-table key holding the server base URL.
const SERVER_URL_KEY: &str = "server_url";

/// Server the app syncs from until another one is configured: the deployed
/// middleware. (The frontend also applies its configured URL on every start.)
pub const DEFAULT_SERVER_URL: &str = "https://lana-mw.heitzli.ch";

/// A cached POI: server data with its geometry as GeoJSON (points and
/// polygons) and the representative point used for bbox filtering.
#[derive(Debug, Clone, Serialize)]
pub struct Poi {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub source: String,
    pub source_id: Option<String>,
    pub lon: f64,
    pub lat: f64,
    pub geometry: serde_json::Value,
    pub properties: serde_json::Value,
}

/// A cached measurement station with its latest reading (both `None` while
/// the station has never reported).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Station {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub source: String,
    pub lon: f64,
    pub lat: f64,
    pub geometry: serde_json::Value,
    pub temperature_c: Option<f64>,
    pub measured_at: Option<DateTime<Utc>>,
}

/// Bounding box in degrees (WGS84): the map's viewport, roughly.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub struct Bbox {
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
}

/// Cache state for the staleness badge: when the last successful sync ran
/// and what snapshot it brought home, plus row counts.
#[derive(Debug, Clone, Serialize)]
pub struct CacheInfo {
    pub last_sync_at: Option<DateTime<Utc>>,
    pub generated_at: Option<DateTime<Utc>>,
    pub poi_count: i64,
    pub station_count: i64,
}

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

/// Replaces the cache wholesale with `snapshot`'s content (ADR 0002: no
/// delta sync): POIs and stations are cleared and re-inserted inside one
/// transaction, and `cache_meta` records the sync time and the snapshot's
/// `generated_at`. Returns the stored counts. Features are parsed before
/// anything is cleared, so a malformed snapshot fails without touching the
/// last good cache content.
pub async fn replace_snapshot(
    pool: &SqlitePool,
    snapshot: &snapshot::Snapshot,
) -> anyhow::Result<(usize, usize)> {
    let pois = snapshot
        .pois
        .features
        .iter()
        .map(poi_row)
        .collect::<anyhow::Result<Vec<_>>>()?;
    let stations = snapshot
        .stations
        .features
        .iter()
        .map(station_row)
        .collect::<anyhow::Result<Vec<_>>>()?;

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM pois").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM stations")
        .execute(&mut *tx)
        .await?;
    for row in &pois {
        sqlx::query(
            "INSERT INTO pois (kind, name, source, source_id, lon, lat, geometry, properties) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.kind)
        .bind(&row.name)
        .bind(&row.source)
        .bind(&row.source_id)
        .bind(row.lon)
        .bind(row.lat)
        .bind(&row.geometry)
        .bind(&row.properties)
        .execute(&mut *tx)
        .await?;
    }
    for row in &stations {
        sqlx::query(
            "INSERT INTO stations (id, kind, name, source, lon, lat, geometry, temperature_c, measured_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.kind)
        .bind(&row.name)
        .bind(&row.source)
        .bind(row.lon)
        .bind(row.lat)
        .bind(&row.geometry)
        .bind(row.temperature_c)
        .bind(row.measured_at)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE cache_meta SET last_sync_at = ?, generated_at = ? WHERE id = 1")
        .bind(Utc::now())
        .bind(snapshot.generated_at)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((pois.len(), stations.len()))
}

/// A POI feature flattened into a storable row (geometry as GeoJSON text,
/// the rest of the properties kept as the raw JSON object).
struct PoiRow {
    kind: String,
    name: String,
    source: String,
    source_id: Option<String>,
    lon: f64,
    lat: f64,
    geometry: String,
    properties: String,
}

fn poi_row(feature: &snapshot::Feature) -> anyhow::Result<PoiRow> {
    let (lon, lat) = feature.representative_point()?;
    Ok(PoiRow {
        kind: feature.required_string("kind")?,
        name: feature.required_string("name")?,
        source: feature.required_string("source")?,
        source_id: feature.optional_string("source_id"),
        lon,
        lat,
        geometry: feature.geometry_json().to_string(),
        properties: serde_json::Value::Object(feature.properties.clone()).to_string(),
    })
}

/// A station feature flattened into a storable row.
struct StationRow {
    id: String,
    kind: String,
    name: String,
    source: String,
    lon: f64,
    lat: f64,
    geometry: String,
    temperature_c: Option<f64>,
    measured_at: Option<DateTime<Utc>>,
}

fn station_row(feature: &snapshot::Feature) -> anyhow::Result<StationRow> {
    let (lon, lat) = feature.representative_point()?;
    let temperature_c = feature
        .property("temperature_c")
        .and_then(serde_json::Value::as_f64);
    let measured_at = match feature
        .property("measured_at")
        .and_then(serde_json::Value::as_str)
    {
        Some(raw) => Some(
            DateTime::parse_from_rfc3339(raw)
                .with_context(|| format!("station measured_at {raw:?} is not RFC 3339"))?
                .with_timezone(&Utc),
        ),
        None => None,
    };
    Ok(StationRow {
        id: feature.required_string("id")?,
        kind: feature.required_string("kind")?,
        name: feature.required_string("name")?,
        source: feature.required_string("source")?,
        lon,
        lat,
        geometry: feature.geometry_json().to_string(),
        temperature_c,
        measured_at,
    })
}

/// Reads the cache state for the staleness badge.
pub async fn cache_info(pool: &SqlitePool) -> anyhow::Result<CacheInfo> {
    let (last_sync_at, generated_at): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT last_sync_at, generated_at FROM cache_meta WHERE id = 1")
            .fetch_one(pool)
            .await?;
    let (poi_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM pois")
        .fetch_one(pool)
        .await?;
    let (station_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM stations")
        .fetch_one(pool)
        .await?;
    Ok(CacheInfo {
        last_sync_at: parse_timestamp(last_sync_at.as_deref())?,
        generated_at: parse_timestamp(generated_at.as_deref())?,
        poi_count,
        station_count,
    })
}

/// Parses a stored RFC 3339 timestamp (`None`/NULL stays `None`).
fn parse_timestamp(raw: Option<&str>) -> anyhow::Result<Option<DateTime<Utc>>> {
    match raw {
        None => Ok(None),
        Some(raw) => Ok(Some(
            DateTime::parse_from_rfc3339(raw)
                .with_context(|| format!("stored timestamp {raw:?} is not RFC 3339"))?
                .with_timezone(&Utc),
        )),
    }
}

/// Lists cached POIs, optionally filtered by `kind` and/or contained in
/// `bbox` (by representative point).
pub async fn list_pois(
    pool: &SqlitePool,
    kind: Option<&str>,
    bbox: Option<Bbox>,
) -> anyhow::Result<Vec<Poi>> {
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            String,
            Option<String>,
            f64,
            f64,
            String,
            String,
        ),
    >(
        "SELECT id, kind, name, source, source_id, lon, lat, geometry, properties \
       FROM pois WHERE (?1 IS NULL OR kind = ?1) \
         AND lon BETWEEN ?2 AND ?4 AND lat BETWEEN ?3 AND ?5 ORDER BY id",
    )
    .bind(kind)
    .bind(bbox.map_or(f64::NEG_INFINITY, |b| b.min_lon))
    .bind(bbox.map_or(f64::NEG_INFINITY, |b| b.min_lat))
    .bind(bbox.map_or(f64::INFINITY, |b| b.max_lon))
    .bind(bbox.map_or(f64::INFINITY, |b| b.max_lat))
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(id, kind, name, source, source_id, lon, lat, geometry, properties)| {
                Ok(Poi {
                    id,
                    kind,
                    name,
                    source,
                    source_id,
                    lon,
                    lat,
                    geometry: serde_json::from_str(&geometry)
                        .context("cached POI geometry is not valid JSON")?,
                    properties: serde_json::from_str(&properties)
                        .context("cached POI properties are not valid JSON")?,
                })
            },
        )
        .collect()
}

/// Lists cached stations (optionally within `bbox`) with their latest
/// temperature reading and its timestamp.
pub async fn list_stations(pool: &SqlitePool, bbox: Option<Bbox>) -> anyhow::Result<Vec<Station>> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            f64,
            f64,
            String,
            Option<f64>,
            Option<String>,
        ),
    >(
        "SELECT id, kind, name, source, lon, lat, geometry, temperature_c, measured_at \
       FROM stations WHERE lon BETWEEN ?1 AND ?3 AND lat BETWEEN ?2 AND ?4 ORDER BY id",
    )
    .bind(bbox.map_or(f64::NEG_INFINITY, |b| b.min_lon))
    .bind(bbox.map_or(f64::NEG_INFINITY, |b| b.min_lat))
    .bind(bbox.map_or(f64::INFINITY, |b| b.max_lon))
    .bind(bbox.map_or(f64::INFINITY, |b| b.max_lat))
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(
            |(id, kind, name, source, lon, lat, geometry, temperature_c, measured_at)| {
                Ok(Station {
                    id,
                    kind,
                    name,
                    source,
                    lon,
                    lat,
                    geometry: serde_json::from_str(&geometry)
                        .context("cached station geometry is not valid JSON")?,
                    temperature_c,
                    measured_at: parse_timestamp(measured_at.as_deref())?,
                })
            },
        )
        .collect()
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
        set_server_url(&pool, "http://192.168.1.42:8090")
            .await
            .expect("valid url accepted");
        assert_eq!(
            get_server_url(&pool).await.unwrap(),
            "http://192.168.1.42:8090"
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

    /// A fixture snapshot: one point POI, one polygon POI, one reporting
    /// station, one silent station.
    fn sample_snapshot() -> snapshot::Snapshot {
        serde_json::from_value(serde_json::json!({
            "pois": {
                "type": "FeatureCollection",
                "features": [
                    {
                        "type": "Feature",
                        "geometry": {"type": "Point", "coordinates": [7.5886, 47.5596]},
                        "properties": {"kind": "fountain", "name": "Marktplatz", "source": "ods-100008", "source_id": "f1"}
                    },
                    {
                        "type": "Feature",
                        "geometry": {"type": "Polygon", "coordinates": [[
                            [8.0, 47.0], [8.2, 47.0], [8.2, 47.2], [8.0, 47.0]
                        ]]},
                        "properties": {"kind": "swim_area", "name": "Rhine swim 1", "source": "ods-100270"}
                    }
                ]
            },
            "stations": {
                "type": "FeatureCollection",
                "features": [
                    {
                        "type": "Feature",
                        "geometry": {"type": "Point", "coordinates": [7.604586, 47.56656]},
                        "properties": {"id": "0020F940", "kind": "air", "name": "Syngenta", "source": "ods-100082", "temperature_c": 31.4, "measured_at": "2026-10-02T14:20:00Z"}
                    },
                    {
                        "type": "Feature",
                        "geometry": {"type": "Point", "coordinates": [7.5947, 47.6014]},
                        "properties": {"id": "rues-s3", "kind": "water", "name": "RUES S3", "source": "ods-100046", "temperature_c": null, "measured_at": null}
                    }
                ]
            },
            "generated_at": "2026-10-02T14:25:00Z"
        }))
        .unwrap()
    }

    /// sync replaces the cache wholesale: pre-existing rows disappear, the
    /// snapshot's rows land with parsed geometry + measurements, and
    /// cache_meta records the sync.
    #[tokio::test]
    async fn replace_snapshot_wipes_old_rows_and_stores_the_snapshot() {
        let (_dir, pool) = test_pool().await;
        // Pre-existing cache content from an earlier sync.
        sqlx::query(
            "INSERT INTO pois (kind, name, source, lon, lat, geometry, properties) \
                     VALUES ('fountain', 'STALE', 'old', 0.0, 0.0, '{}', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO stations (id, kind, name, source, lon, lat, geometry) \
                     VALUES ('stale-station', 'air', 'STALE', 'old', 0.0, 0.0, '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let (pois, stations) = replace_snapshot(&pool, &sample_snapshot())
            .await
            .expect("replace_snapshot failed");
        assert_eq!((pois, stations), (2, 2));

        let info = cache_info(&pool).await.unwrap();
        assert_eq!(info.poi_count, 2, "stale POI gone");
        assert_eq!(info.station_count, 2, "stale station gone");
        let last_sync = info.last_sync_at.expect("sync time recorded");
        assert!(Utc::now() - last_sync < chrono::Duration::seconds(30));
        assert_eq!(
            info.generated_at,
            Some(
                DateTime::parse_from_rfc3339("2026-10-02T14:25:00Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );

        // Round-trip through the public query seam.
        let all = list_pois(&pool, None, None).await.unwrap();
        assert_eq!(all.len(), 2);
        let fountain = all.iter().find(|p| p.kind == "fountain").unwrap();
        assert_eq!(fountain.name, "Marktplatz");
        assert_eq!((fountain.lon, fountain.lat), (7.5886, 47.5596));
        assert_eq!(fountain.geometry["type"], "Point");
        let swim = all.iter().find(|p| p.kind == "swim_area").unwrap();
        assert_eq!(swim.geometry["type"], "Polygon");
        assert!((swim.lon - 8.1).abs() < 1e-9, "polygon mean lon");

        let stations = list_stations(&pool, None).await.unwrap();
        assert_eq!(stations.len(), 2);
        let reporting = stations.iter().find(|s| s.id == "0020F940").unwrap();
        assert_eq!(reporting.temperature_c, Some(31.4));
        assert_eq!(
            reporting.measured_at,
            Some(
                DateTime::parse_from_rfc3339("2026-10-02T14:20:00Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
        let silent = stations.iter().find(|s| s.id == "rues-s3").unwrap();
        assert_eq!(silent.temperature_c, None);
        assert_eq!(silent.measured_at, None);
    }

    /// POI filters: by kind, by bbox (representative point inside), and the
    /// combination; a station bbox works the same way.
    #[tokio::test]
    async fn poi_and_station_filters_by_kind_and_bbox() {
        let (_dir, pool) = test_pool().await;
        replace_snapshot(&pool, &sample_snapshot()).await.unwrap();

        let basel = Bbox {
            min_lon: 7.0,
            min_lat: 47.0,
            max_lon: 8.0,
            max_lat: 48.0,
        };
        let east = Bbox {
            min_lon: 8.0,
            min_lat: 47.0,
            max_lon: 9.0,
            max_lat: 48.0,
        };

        assert_eq!(
            list_pois(&pool, Some("fountain"), None)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            list_pois(&pool, Some("cool_place"), None)
                .await
                .unwrap()
                .len(),
            0
        );
        let in_basel = list_pois(&pool, None, Some(basel)).await.unwrap();
        assert_eq!(in_basel.len(), 1, "only the Basel fountain is inside");
        assert_eq!(in_basel[0].name, "Marktplatz");
        assert_eq!(
            list_pois(&pool, None, Some(east)).await.unwrap().len(),
            1,
            "the polygon's mean point is east"
        );
        assert_eq!(
            list_pois(&pool, Some("swim_area"), Some(east))
                .await
                .unwrap()
                .len(),
            1,
            "kind and bbox combine"
        );
        assert_eq!(
            list_pois(&pool, Some("fountain"), Some(east))
                .await
                .unwrap()
                .len(),
            0
        );

        let stations_east = list_stations(&pool, Some(east)).await.unwrap();
        assert_eq!(stations_east.len(), 0, "no stations east");
        let stations_basel = list_stations(&pool, Some(basel)).await.unwrap();
        assert_eq!(stations_basel.len(), 2);
    }

    /// A cache that has never synced reports no sync time and empty counts.
    #[tokio::test]
    async fn cache_info_before_first_sync_is_empty() {
        let (_dir, pool) = test_pool().await;
        let info = cache_info(&pool).await.unwrap();
        assert_eq!(info.last_sync_at, None);
        assert_eq!(info.generated_at, None);
        assert_eq!(info.poi_count, 0);
        assert_eq!(info.station_count, 0);
    }
}
