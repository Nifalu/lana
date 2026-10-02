//! `GET /api/v1/snapshot` – full offline-sync snapshot.
//!
//! The snapshot is the app's whole world: every POI and every station as
//! GeoJSON, replaced wholesale into the on-device cache. POI features carry
//! `kind`, `name`, `source` (plus `source_id` and the stored source-specific
//! properties, e.g. cool-place address/note/url); station features carry
//! `kind`, `name`, `source` and the station `id`. Geometry is served exactly
//! as stored by PostGIS (`ST_AsGeoJSON`, `[lon, lat]`), so swim-area polygons
//! keep their shape.

use axum::extract::State;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;

use crate::geojson::{Feature, FeatureCollection, Geometry};
use sqlx::PgPool;

/// The full-snapshot response document.
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub pois: FeatureCollection,
    pub stations: FeatureCollection,
    pub generated_at: DateTime<Utc>,
}

pub async fn get_snapshot(State(pool): State<PgPool>) -> Result<Json<Snapshot>, super::ApiError> {
    let pois = load_pois(&pool).await?;
    let stations = load_stations(&pool).await?;
    Ok(Json(Snapshot {
        pois,
        stations,
        generated_at: Utc::now(),
    }))
}

/// Loads every POI as a GeoJSON feature with kind/name/source merged into the
/// stored properties.
async fn load_pois(pool: &PgPool) -> Result<FeatureCollection, super::ApiError> {
    let rows = sqlx::query_as::<
        _,
        (
            String,         // kind
            String,         // name
            String,         // source
            Option<String>, // source_id
            String,         // properties (jsonb as text)
            String,         // ST_AsGeoJSON(geom)
        ),
    >(
        "SELECT kind, name, source, source_id, properties::text, \
                COALESCE(ST_AsGeoJSON(geom), '') \
         FROM pois ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    let features = rows
        .into_iter()
        .map(|(kind, name, source, source_id, properties, geojson)| {
            let geometry: Geometry = serde_json::from_str(&geojson)
                .map_err(|e| anyhow::anyhow!("stored geometry is not valid GeoJSON: {e}"))?;
            let mut properties: serde_json::Map<String, serde_json::Value> =
                serde_json::from_str(&properties)
                    .map_err(|e| anyhow::anyhow!("stored properties are not valid JSON: {e}"))?;
            properties.insert("kind".to_string(), json!(kind));
            properties.insert("name".to_string(), json!(name));
            properties.insert("source".to_string(), json!(source));
            if let Some(source_id) = source_id {
                properties.insert("source_id".to_string(), json!(source_id));
            }
            Ok(Feature::new(geometry, properties))
        })
        .collect::<Result<Vec<_>, super::ApiError>>()?;
    Ok(FeatureCollection::new(features))
}

/// Loads every station as a GeoJSON feature; the station `id` doubles as the
/// join key for later measurement tickets.
async fn load_stations(pool: &PgPool) -> Result<FeatureCollection, super::ApiError> {
    let rows = sqlx::query_as::<
        _,
        (
            String, // id
            String, // kind
            String, // name
            String, // source
            String, // ST_AsGeoJSON(geom)
        ),
    >(
        "SELECT id, kind, name, source, COALESCE(ST_AsGeoJSON(geom), '') \
         FROM stations ORDER BY id",
    )
    .fetch_all(pool)
    .await?;

    let features = rows
        .into_iter()
        .map(|(id, kind, name, source, geojson)| {
            let geometry: Geometry = serde_json::from_str(&geojson)
                .map_err(|e| anyhow::anyhow!("stored geometry is not valid GeoJSON: {e}"))?;
            let mut properties = serde_json::Map::new();
            properties.insert("id".to_string(), json!(id));
            properties.insert("kind".to_string(), json!(kind));
            properties.insert("name".to_string(), json!(name));
            properties.insert("source".to_string(), json!(source));
            Ok(Feature::new(geometry, properties))
        })
        .collect::<Result<Vec<_>, super::ApiError>>()?;
    Ok(FeatureCollection::new(features))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::router;
    use crate::import;
    use crate::import::ImportSummary;
    use crate::test_support::{self, FixtureSource};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn get_snapshot_json(app: &axum::Router) -> serde_json::Value {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/snapshot")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn find_feature<'a>(
        features: &'a [serde_json::Value],
        property: &str,
        value: &str,
    ) -> &'a serde_json::Value {
        features
            .iter()
            .find(|f| f["properties"][property] == *value)
            .unwrap_or_else(|| panic!("no feature with {property} = {value:?}"))
    }

    /// The primary seam, end to end over the HTTP API: import fills the
    /// snapshot with real POIs and stations (GeoJSON, `[lon, lat]`, kind/name/
    /// source), swim polygons survive, cool places carry address/note/url, and
    /// a re-import duplicates nothing while updating changed rows in place.
    #[tokio::test]
    async fn import_fills_the_snapshot_and_reimport_is_idempotent() {
        let Some(pool) = test_support::db_pool().await else {
            return;
        };
        let _db = test_support::lock_db().await;
        test_support::reset_db(&pool).await;
        let app = router(pool.clone());

        // An empty database still serves a valid, empty snapshot.
        let json = get_snapshot_json(&app).await;
        assert_eq!(json["pois"]["type"], "FeatureCollection");
        assert_eq!(json["pois"]["features"], serde_json::json!([]));
        assert_eq!(json["stations"]["features"], serde_json::json!([]));

        // Import the fixture datasets.
        let summary = import::run(&pool, &FixtureSource::default()).await.unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                fountains: 3,
                swim_areas: 4,
                cool_places: 20,
                air_stations: 10
            }
        );

        let json = get_snapshot_json(&app).await;
        let pois = json["pois"]["features"].as_array().unwrap();
        assert_eq!(
            pois.len(),
            27,
            "3 fountains + 4 swim areas + 20 cool places"
        );

        // Fountains: Point geometry in [lon, lat] order with kind/name/source.
        let fountain = find_feature(pois, "name", "Spritz-Brunnen");
        assert_eq!(fountain["properties"]["kind"], "fountain");
        assert_eq!(fountain["properties"]["source"], "ods-100008");
        assert_eq!(
            fountain["properties"]["source_id"].as_str().unwrap().len(),
            16
        );
        assert_eq!(fountain["geometry"]["type"], "Point");
        assert_eq!(
            fountain["geometry"]["coordinates"],
            serde_json::json!([7.5900145, 47.5704258])
        );
        assert_eq!(
            fountain["properties"]["desc"],
            "Dreirosenanlage / Klybeckstrasse (Foto Christian Lienhard, Basel)"
        );

        // Swim areas: the polygon shape is preserved for map drawing.
        let swim = find_feature(pois, "name", "Rhine swim area 1");
        assert_eq!(swim["properties"]["kind"], "swim_area");
        assert_eq!(swim["properties"]["source"], "ods-100270");
        assert_eq!(swim["geometry"]["type"], "Polygon");
        assert_eq!(
            swim["geometry"]["coordinates"][0].as_array().unwrap().len(),
            73,
            "outer ring of swim area 1 has 73 vertices in the fixture"
        );

        // Cool places carry address/note/url.
        let museum = find_feature(pois, "name", "Kunstmuseum Basel Hauptbau");
        assert_eq!(museum["properties"]["kind"], "cool_place");
        assert_eq!(museum["properties"]["source"], "seed-cool-places");
        assert_eq!(
            museum["properties"]["address"],
            "St. Alban-Graben 8, 4010 Basel"
        );
        assert!(museum["properties"]["note"]
            .as_str()
            .unwrap()
            .contains("25"));
        assert_eq!(museum["properties"]["url"], "https://kunstmuseumbasel.ch");
        assert_eq!(
            museum["geometry"]["coordinates"],
            serde_json::json!([7.5944513, 47.5539294])
        );

        // Stations: air stations with kind/name/source and the station id.
        let stations = json["stations"]["features"].as_array().unwrap();
        assert_eq!(stations.len(), 10, "two record pages of 5 fixture rows");
        let station = find_feature(stations, "id", "0020F940");
        assert_eq!(station["properties"]["kind"], "air");
        assert_eq!(station["properties"]["name"], "Syngenta");
        assert_eq!(station["properties"]["source"], "ods-100082");
        assert_eq!(
            station["geometry"]["coordinates"],
            serde_json::json!([7.604586, 47.56656])
        );

        // Re-import the same data: nothing duplicates.
        import::run(&pool, &FixtureSource::default()).await.unwrap();
        let json = get_snapshot_json(&app).await;
        assert_eq!(json["pois"]["features"].as_array().unwrap().len(), 27);
        assert_eq!(json["stations"]["features"].as_array().unwrap().len(), 10);

        // Changed data with the same identity updates in place instead of
        // duplicating (fountain source ids hash name + position, so the
        // description changes without moving the row).
        let mut changed = FixtureSource::default();
        changed.fountains = changed.fountains.replace(
            "Dreirosenanlage / Klybeckstrasse (Foto Christian Lienhard, Basel)",
            "Neue Beschreibung",
        );
        import::run(&pool, &changed).await.unwrap();
        let json = get_snapshot_json(&app).await;
        let pois = json["pois"]["features"].as_array().unwrap();
        assert_eq!(pois.len(), 27, "updated rows must not duplicate");
        let fountain = find_feature(pois, "name", "Spritz-Brunnen");
        assert_eq!(fountain["properties"]["desc"], "Neue Beschreibung");

        pool.close().await;
    }
}
