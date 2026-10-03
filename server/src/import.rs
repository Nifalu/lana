//! Import of Basel open data into the database.
//!
//! Split in two halves:
//! - **Pure parsing**: dataset exports (GeoJSON) and record pages (JSON) are
//!   turned into insert-ready rows by pure functions. These are tested against
//!   the committed fixture files in `testdata/` - no network involved.
//! - **Storage**: `upsert_pois` / `upsert_stations` write rows with upsert
//!   semantics keyed on `(source, source_id)` (stations: `id`), so re-running
//!   the import never duplicates rows; changed data updates in place.
//!
//! `run` glues the two together for a given [`crate::ods::DatasetSource`].
//!
//! Source-id notes (the Opendatasoft datasets expose **no record ids at all**):
//! - fountains: ids are synthesized as a SHA-256 prefix over name + position
//!   (names are heavily duplicated, so the position disambiguates);
//! - swim areas: SHA-256 prefix over the dataset-provided centroid position;
//! - cool places: the curated seed's names are unique, so the name is the id;
//! - air stations: `name_original` is the stable station id.
//!
//! All of these are deterministic, so re-imports upsert instead of duplicating.

use anyhow::Context;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::geojson::{Geometry, Position};
use crate::ods::DatasetSource;

/// POI kinds (mirrors the `pois.kind` CHECK constraint).
pub const KIND_FOUNTAIN: &str = "fountain";
pub const KIND_SWIM_AREA: &str = "swim_area";
pub const KIND_COOL_PLACE: &str = "cool_place";
/// Station kinds (mirrors the `stations.kind` CHECK constraint).
pub const KIND_AIR: &str = "air";

/// `source` values: which dataset a row came from.
pub const SOURCE_FOUNTAINS: &str = "ods-100008";
pub const SOURCE_SWIM_AREAS: &str = "ods-100270";
pub const SOURCE_COOL_PLACES: &str = "seed-cool-places";
pub const SOURCE_AIR_STATIONS: &str = "ods-100082";

/// The committed cool-places seed, embedded in the binary. Addresses were
/// geocoded once with Nominatim when the seed was authored - never at runtime
/// (see `seed/README.md`).
pub const COOL_PLACES_SEED: &str = include_str!("../seed/cool-places.geojson");

/// One POI row ready for upsert. `geom` keeps the source GeoJSON geometry
/// (Point or Polygon), so swim-area shapes survive the trip to the database.
#[derive(Debug, Clone, PartialEq)]
pub struct PoiInsert {
    pub kind: &'static str,
    pub name: String,
    pub geom: Geometry,
    pub properties: serde_json::Map<String, serde_json::Value>,
    pub source: &'static str,
    pub source_id: String,
}

/// One station row ready for upsert.
#[derive(Debug, Clone, PartialEq)]
pub struct StationInsert {
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    pub lon: f64,
    pub lat: f64,
    pub source: &'static str,
}

/// How many rows each import step loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportSummary {
    pub fountains: usize,
    pub swim_areas: usize,
    pub cool_places: usize,
    pub air_stations: usize,
}

/// Fetches every dataset from `source`, parses it and upserts it.
pub(crate) async fn run(
    pool: &PgPool,
    source: &impl DatasetSource,
) -> anyhow::Result<ImportSummary> {
    let fountains = parse_fountains(&source.fountains().await?)?;
    upsert_pois(pool, &fountains).await?;

    let swim_areas = parse_swim_areas(&source.swim_areas().await?)?;
    upsert_pois(pool, &swim_areas).await?;

    let cool_places = parse_cool_places(COOL_PLACES_SEED)?;
    upsert_pois(pool, &cool_places).await?;

    let pages = source.air_stations().await?;
    let page_refs: Vec<&str> = pages.iter().map(String::as_str).collect();
    let air_stations = parse_air_stations(&page_refs)?;
    upsert_stations(pool, &air_stations).await?;

    Ok(ImportSummary {
        fountains: fountains.len(),
        swim_areas: swim_areas.len(),
        cool_places: cool_places.len(),
        air_stations: air_stations.len(),
    })
}

// ---------------------------------------------------------------------------
// Pure parsing
// ---------------------------------------------------------------------------

/// The GeoJSON export shape used by data.bs.ch: a FeatureCollection whose
/// features carry a standard geometry plus arbitrary properties. Our own
/// `Geometry` type round-trips this exactly (tagged `"type"`, `[lon, lat]`).
#[derive(Deserialize)]
struct RawFeatureCollection {
    features: Vec<RawFeature>,
}

#[derive(Deserialize)]
struct RawFeature {
    geometry: Geometry,
    properties: serde_json::Map<String, serde_json::Value>,
}

/// Parses the fountains GeoJSON export (dataset 100008). All fountains are
/// `kind = fountain` - the dataset has no machine-readable type field; media
/// blobs (`gx_media_links`, `picture_link`) are discarded.
pub fn parse_fountains(raw: &str) -> anyhow::Result<Vec<PoiInsert>> {
    let fc: RawFeatureCollection =
        serde_json::from_str(raw).context("fountains export is not valid GeoJSON")?;
    fc.features.into_iter().map(fountain_poi).collect()
}

fn fountain_poi(feature: RawFeature) -> anyhow::Result<PoiInsert> {
    let name = prop_string(&feature.properties, "name").unwrap_or_else(|| "Fountain".to_string());
    let position = point_of(&feature.geometry)
        .with_context(|| format!("fountain {name:?} has no point geometry"))?;

    let mut properties = serde_json::Map::new();
    if let Some(desc) = prop_string(&feature.properties, "desc") {
        properties.insert("desc".to_string(), serde_json::Value::String(desc));
    }

    // Ids must not collide for equal names - mix in the position.
    let source_id = stable_id(&format!("100008|{name}|{}|{}", position.0, position.1));

    Ok(PoiInsert {
        kind: KIND_FOUNTAIN,
        name,
        geom: Geometry::Point {
            coordinates: position,
        },
        properties,
        source: SOURCE_FOUNTAINS,
        source_id,
    })
}

/// Parses the Rhine swim-areas GeoJSON export (dataset 100270): 4 Polygon
/// features with no properties besides a `geo_point_2d` centroid. Names and
/// ids are synthesized deterministically: features are numbered by centroid
/// position ("Rhine swim area 1..n"), ids hash the centroid.
pub fn parse_swim_areas(raw: &str) -> anyhow::Result<Vec<PoiInsert>> {
    let fc: RawFeatureCollection =
        serde_json::from_str(raw).context("swim-areas export is not valid GeoJSON")?;

    let mut areas = Vec::new();
    for feature in fc.features {
        let rings = match &feature.geometry {
            Geometry::Polygon { coordinates } => coordinates.clone(),
            other => anyhow::bail!("swim area has non-polygon geometry: {other:?}"),
        };
        let centroid = geo_point_2d(&feature.properties).unwrap_or_else(|| {
            // Fallback: average of the outer ring if the export omits it.
            let (mut sx, mut sy) = (0.0, 0.0);
            let ring = &rings[0];
            for p in ring {
                sx += p.0;
                sy += p.1;
            }
            (sx / ring.len() as f64, sy / ring.len() as f64)
        });
        areas.push((centroid, rings));
    }
    // Deterministic numbering: order by centroid before assigning names/ids.
    areas.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    Ok(areas
        .into_iter()
        .enumerate()
        .map(|(index, (centroid, rings))| {
            let name = format!("Rhine swim area {}", index + 1);
            let source_id = stable_id(&format!("100270|{}|{}", centroid.0, centroid.1));
            PoiInsert {
                kind: KIND_SWIM_AREA,
                name,
                geom: Geometry::Polygon { coordinates: rings },
                properties: serde_json::Map::new(),
                source: SOURCE_SWIM_AREAS,
                source_id,
            }
        })
        .collect())
}

/// Parses the committed cool-places seed: Point features with name, address,
/// note and url properties. Names in the curated seed are unique, so the name
/// doubles as source id.
pub fn parse_cool_places(raw: &str) -> anyhow::Result<Vec<PoiInsert>> {
    let fc: RawFeatureCollection =
        serde_json::from_str(raw).context("cool-places seed is not valid GeoJSON")?;
    fc.features
        .into_iter()
        .map(|feature| {
            let name = prop_string(&feature.properties, "name")
                .context("cool place without a name in seed")?;
            let position = point_of(&feature.geometry)
                .with_context(|| format!("cool place {name:?} has no point geometry"))?;
            let mut properties = serde_json::Map::new();
            for key in ["address", "note", "url"] {
                if let Some(value) = prop_string(&feature.properties, key) {
                    properties.insert(key.to_string(), serde_json::Value::String(value));
                }
            }
            Ok(PoiInsert {
                kind: KIND_COOL_PLACE,
                source_id: name.clone(),
                name,
                geom: Geometry::Point {
                    coordinates: position,
                },
                properties,
                source: SOURCE_COOL_PLACES,
            })
        })
        .collect()
}

/// Parses air-station record pages (dataset 100082). Each page is a
/// `{total_count, results: [...]}` document with flat records; pages are
/// concatenated in order.
pub fn parse_air_stations(pages: &[&str]) -> anyhow::Result<Vec<StationInsert>> {
    let mut stations = Vec::new();
    for page in pages {
        let value: serde_json::Value =
            serde_json::from_str(page).context("air-stations page is not valid JSON")?;
        let results = value
            .get("results")
            .and_then(|r| r.as_array())
            .context("air-stations page has no results array")?;
        for record in results {
            let id = record
                .get("name_original")
                .and_then(|v| v.as_str())
                .context("air-station record without name_original")?
                .to_string();
            let name = record
                .get("name_custom")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string();
            let (lon, lat) = record_coords(record)
                .with_context(|| format!("air station {id:?} without coordinates"))?;
            stations.push(StationInsert {
                id,
                kind: KIND_AIR,
                name,
                lon,
                lat,
                source: SOURCE_AIR_STATIONS,
            });
        }
    }
    Ok(stations)
}

/// Station coordinates: `coords: {lon, lat}` object, falling back to the plain
/// `lon`/`lat` doubles that dataset 100082 additionally duplicates.
fn record_coords(record: &serde_json::Value) -> Option<(f64, f64)> {
    let lon = record["coords"]["lon"]
        .as_f64()
        .or_else(|| record["lon"].as_f64())?;
    let lat = record["coords"]["lat"]
        .as_f64()
        .or_else(|| record["lat"].as_f64())?;
    Some((lon, lat))
}

fn point_of(geometry: &Geometry) -> Option<Position> {
    match geometry {
        Geometry::Point { coordinates } => Some(*coordinates),
        _ => None,
    }
}

fn prop_string(
    properties: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    properties
        .get(key)
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn geo_point_2d(properties: &serde_json::Map<String, serde_json::Value>) -> Option<(f64, f64)> {
    let lon = properties.get("geo_point_2d")?["lon"].as_f64()?;
    let lat = properties.get("geo_point_2d")?["lat"].as_f64()?;
    Some((lon, lat))
}

/// Deterministic short hash (first 8 bytes of SHA-256, hex) for synthesized
/// source ids. Stable across runs and binary versions.
fn stable_id(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut hex = String::with_capacity(16);
    for byte in &digest[..8] {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

// ---------------------------------------------------------------------------
// Storage (upserts)
// ---------------------------------------------------------------------------

/// Upserts POIs keyed on `(source, source_id)`: re-running never duplicates,
/// changed rows update in place. Geometry travels as GeoJSON and is parsed by
/// PostGIS, so Points and Polygons take the same code path.
pub async fn upsert_pois(pool: &PgPool, pois: &[PoiInsert]) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for poi in pois {
        let geom = serde_json::to_string(&poi.geom)?;
        let properties = serde_json::to_string(&poi.properties)?;
        sqlx::query(
            "INSERT INTO pois (kind, name, geom, properties, source, source_id) \
             VALUES ($1, $2, ST_SetSRID(ST_GeomFromGeoJSON($3), 4326)::geography, $4::jsonb, $5, $6) \
             ON CONFLICT (source, source_id) DO UPDATE SET \
                 kind = EXCLUDED.kind, name = EXCLUDED.name, \
                 geom = EXCLUDED.geom, properties = EXCLUDED.properties",
        )
        .bind(poi.kind)
        .bind(&poi.name)
        .bind(&geom)
        .bind(&properties)
        .bind(poi.source)
        .bind(&poi.source_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Upserts stations keyed on their id (the dataset's stable station id).
pub async fn upsert_stations(pool: &PgPool, stations: &[StationInsert]) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for station in stations {
        sqlx::query(
            "INSERT INTO stations (id, kind, name, geom, source) \
             VALUES ($1, $2, $3, ST_SetSRID(ST_MakePoint($4, $5), 4326)::geography, $6) \
             ON CONFLICT (id) DO UPDATE SET \
                 kind = EXCLUDED.kind, name = EXCLUDED.name, \
                 geom = EXCLUDED.geom, source = EXCLUDED.source",
        )
        .bind(&station.id)
        .bind(station.kind)
        .bind(&station.name)
        .bind(station.lon)
        .bind(station.lat)
        .bind(station.source)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const FOUNTAINS_FIXTURE: &str = include_str!("../testdata/fountains.geojson");
    const SWIM_AREAS_FIXTURE: &str = include_str!("../testdata/swim-areas.geojson");
    const AIR_STATIONS_FIXTURE: &str = include_str!("../testdata/air-stations.json");
    const AIR_STATIONS_PAGE2_FIXTURE: &str = include_str!("../testdata/air-stations-page2.json");

    /// Fountains parse from the committed export fixture: name, point
    /// geometry in [lon, lat], description kept, media blobs dropped.
    #[test]
    fn parses_fountains_fixture() {
        let pois = parse_fountains(FOUNTAINS_FIXTURE).unwrap();
        assert_eq!(pois.len(), 3);

        let first = &pois[0];
        assert_eq!(first.kind, KIND_FOUNTAIN);
        assert_eq!(first.source, SOURCE_FOUNTAINS);
        assert_eq!(first.name, "Spritz-Brunnen");
        assert_eq!(
            first.geom,
            Geometry::Point {
                coordinates: Position(7.5900145, 47.5704258)
            }
        );
        assert_eq!(
            first.properties["desc"],
            "Dreirosenanlage / Klybeckstrasse (Foto Christian Lienhard, Basel)"
        );
        // No media blobs leak into the properties.
        assert!(first.properties.get("gx_media_links").is_none());
        assert!(first.properties.get("picture_link").is_none());
    }

    /// Synthesized fountain ids are stable and never collide for equal names
    /// at different places (30 features share the name "Basilisk" upstream).
    #[test]
    fn fountain_source_ids_are_stable_and_position_sensitive() {
        let pois = parse_fountains(FOUNTAINS_FIXTURE).unwrap();
        assert_eq!(pois[0].source_id.len(), 16);
        assert_eq!(
            pois[0].source_id,
            parse_fountains(FOUNTAINS_FIXTURE).unwrap()[0].source_id
        );

        let same_name = |lon: f64, lat: f64| {
            let raw = serde_json::json!({
                "type": "FeatureCollection",
                "features": [{
                    "type": "Feature",
                    "geometry": {"type": "Point", "coordinates": [lon, lat]},
                    "properties": {"name": "Basilisk"}
                }]
            });
            parse_fountains(&raw.to_string())
                .unwrap()
                .remove(0)
                .source_id
        };
        assert_ne!(same_name(7.5, 47.5), same_name(7.6, 47.6));
    }

    /// Swim areas keep their polygon geometry and get deterministic names and
    /// ids (the dataset has neither).
    #[test]
    fn parses_swim_areas_fixture_with_polygons() {
        let pois = parse_swim_areas(SWIM_AREAS_FIXTURE).unwrap();
        assert_eq!(pois.len(), 4);
        assert!(pois
            .iter()
            .all(|p| p.kind == KIND_SWIM_AREA && p.source == SOURCE_SWIM_AREAS));

        // Numbered by ascending centroid position.
        assert_eq!(pois[0].name, "Rhine swim area 1");
        assert_eq!(pois[3].name, "Rhine swim area 4");
        let ids: HashSet<&str> = pois.iter().map(|p| p.source_id.as_str()).collect();
        assert_eq!(ids.len(), 4);

        // The first area's polygon survived: 73 outer-ring vertices in the
        // fixture (verified against the raw export).
        match &pois[0].geom {
            Geometry::Polygon { coordinates } => assert_eq!(coordinates[0].len(), 73),
            other => panic!("expected polygon, got {other:?}"),
        }
    }

    /// The committed cool-places seed parses with address/note/url and the
    /// name as source id.
    #[test]
    fn parses_cool_places_seed() {
        let pois = parse_cool_places(COOL_PLACES_SEED).unwrap();
        assert_eq!(pois.len(), 20);

        let museum = pois
            .iter()
            .find(|p| p.name == "Kunstmuseum Basel Hauptbau")
            .expect("seed contains the Kunstmuseum");
        assert_eq!(museum.kind, KIND_COOL_PLACE);
        assert_eq!(museum.source, SOURCE_COOL_PLACES);
        assert_eq!(museum.source_id, "Kunstmuseum Basel Hauptbau");
        assert_eq!(
            museum.geom,
            Geometry::Point {
                coordinates: Position(7.5944513, 47.5539294)
            }
        );
        assert_eq!(
            museum.properties["address"],
            "St. Alban-Graben 8, 4010 Basel"
        );
        assert_eq!(museum.properties["url"], "https://kunstmuseumbasel.ch");
        assert!(museum.properties["note"].as_str().unwrap().contains("25"));
    }

    /// Air-station record pages parse into stations keyed by name_original,
    /// with name_custom as display name and coords in [lon, lat] order.
    #[test]
    fn parses_air_station_pages_fixture() {
        let stations =
            parse_air_stations(&[AIR_STATIONS_FIXTURE, AIR_STATIONS_PAGE2_FIXTURE]).unwrap();
        assert_eq!(stations.len(), 10);

        let first = &stations[0];
        assert_eq!(first.id, "0020F940");
        assert_eq!(first.name, "Syngenta");
        assert_eq!(first.kind, KIND_AIR);
        assert_eq!(first.source, SOURCE_AIR_STATIONS);
        assert_eq!(first.lon, 7.604586);
        assert_eq!(first.lat, 47.56656);

        let ids: HashSet<&str> = stations.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids.len(), 10, "pages must not overlap");
    }

    /// A station without name_custom falls back to the station id as name.
    #[test]
    fn air_station_without_custom_name_falls_back_to_id() {
        let raw = serde_json::json!({
            "total_count": 1,
            "results": [{
                "name_original": "TEST0001",
                "coords": {"lon": 7.5, "lat": 47.5}
            }]
        });
        let stations = parse_air_stations(&[&raw.to_string()]).unwrap();
        assert_eq!(stations[0].name, "TEST0001");
    }
}
