//! Client-side types for the server's `GET /api/v1/snapshot` document.
//!
//! Mirrors the server's wire format: `pois` and `stations` as GeoJSON
//! FeatureCollections (coordinate order `[lon, lat]`) plus `generated_at`.
//! Geometry is kept as raw JSON (the cache serves it back to the map
//! unchanged); the representative point is extracted for bbox filtering.

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

/// The full-snapshot response document.
#[derive(Debug, Clone, Deserialize)]
pub struct Snapshot {
    pub pois: FeatureCollection,
    pub stations: FeatureCollection,
    pub generated_at: DateTime<Utc>,
}

/// A GeoJSON FeatureCollection.
#[derive(Debug, Clone, Deserialize)]
pub struct FeatureCollection {
    pub features: Vec<Feature>,
}

/// A GeoJSON Feature: geometry plus its properties (kind/name/source and,
/// for stations, id + latest measurement).
#[derive(Debug, Clone, Deserialize)]
pub struct Feature {
    pub geometry: Value,
    #[serde(default)]
    pub properties: serde_json::Map<String, Value>,
}

impl Feature {
    /// A property of the feature, if present.
    pub fn property(&self, key: &str) -> Option<&Value> {
        self.properties.get(key)
    }

    /// String property, erroring when absent – kind/name/source/id are
    /// structural and a snapshot without them is broken data.
    pub fn required_string(&self, key: &str) -> anyhow::Result<String> {
        match self.properties.get(key) {
            Some(Value::String(s)) => Ok(s.clone()),
            other => anyhow::bail!("feature property {key:?} must be a string, got {other:?}"),
        }
    }

    /// Optional string property.
    pub fn optional_string(&self, key: &str) -> Option<String> {
        self.properties
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    /// The geometry as a JSON value (stored and served back unchanged).
    pub fn geometry_json(&self) -> &Value {
        &self.geometry
    }

    /// The feature's representative point: the coordinate itself for Points,
    /// the mean of the outer ring for Polygons (bbox filtering is
    /// approximate there – good enough to decide what the map loads).
    pub fn representative_point(&self) -> anyhow::Result<(f64, f64)> {
        let kind = self
            .geometry
            .get("type")
            .and_then(Value::as_str)
            .context("geometry without a type")?;
        match kind {
            "Point" => {
                let coords = self
                    .geometry
                    .get("coordinates")
                    .context("Point without coordinates")?;
                let (lon, lat) = lon_lat(coords)?;
                Ok((lon, lat))
            }
            "Polygon" => {
                let outer = self
                    .geometry
                    .get("coordinates")
                    .and_then(Value::as_array)
                    .and_then(|rings| rings.first())
                    .and_then(Value::as_array)
                    .context("Polygon without an outer ring")?;
                anyhow::ensure!(!outer.is_empty(), "Polygon with an empty outer ring");
                let mut sum_lon = 0.0;
                let mut sum_lat = 0.0;
                for position in outer {
                    let (lon, lat) = lon_lat(position)?;
                    sum_lon += lon;
                    sum_lat += lat;
                }
                let n = outer.len() as f64;
                Ok((sum_lon / n, sum_lat / n))
            }
            other => anyhow::bail!("unsupported geometry type {other:?}"),
        }
    }
}

/// Reads a GeoJSON position pair `[lon, lat]` (longitude first, RFC 7946).
fn lon_lat(position: &Value) -> anyhow::Result<(f64, f64)> {
    let pair = position.as_array().context("position must be an array")?;
    anyhow::ensure!(pair.len() >= 2, "position must have at least two values");
    let lon = pair[0].as_f64().context("longitude must be a number")?;
    let lat = pair[1].as_f64().context("latitude must be a number")?;
    Ok((lon, lat))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A minimal but complete snapshot document (the shape the server
    /// serves) parses into typed features with the right properties.
    #[test]
    fn parses_full_snapshot_document() {
        let document = json!({
            "pois": {
                "type": "FeatureCollection",
                "features": [
                    {
                        "type": "Feature",
                        "geometry": {"type": "Point", "coordinates": [7.5886, 47.5596]},
                        "properties": {
                            "kind": "fountain",
                            "name": "Spritz-Brunnen",
                            "source": "ods-100008",
                            "source_id": "abc"
                        }
                    },
                    {
                        "type": "Feature",
                        "geometry": {"type": "Polygon", "coordinates": [[
                            [7.5, 47.5], [7.6, 47.5], [7.6, 47.6], [7.5, 47.5]
                        ]]},
                        "properties": {
                            "kind": "swim_area",
                            "name": "Rhine swim area 1",
                            "source": "ods-100270"
                        }
                    }
                ]
            },
            "stations": {
                "type": "FeatureCollection",
                "features": [
                    {
                        "type": "Feature",
                        "geometry": {"type": "Point", "coordinates": [7.604586, 47.56656]},
                        "properties": {
                            "id": "0020F940",
                            "kind": "air",
                            "name": "Syngenta",
                            "source": "ods-100082",
                            "temperature_c": 31.4,
                            "measured_at": "2026-10-02T14:20:00Z"
                        }
                    },
                    {
                        "type": "Feature",
                        "geometry": {"type": "Point", "coordinates": [7.59, 47.57]},
                        "properties": {
                            "id": "never-reported",
                            "kind": "water",
                            "name": "RUES S3",
                            "source": "ods-100046",
                            "temperature_c": null,
                            "measured_at": null
                        }
                    }
                ]
            },
            "generated_at": "2026-10-02T14:25:00Z"
        });

        let snapshot: Snapshot = serde_json::from_value(document).expect("snapshot parses");
        assert_eq!(snapshot.pois.features.len(), 2);
        assert_eq!(snapshot.stations.features.len(), 2);
        assert_eq!(
            snapshot.generated_at,
            DateTime::parse_from_rfc3339("2026-10-02T14:25:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );

        let fountain = &snapshot.pois.features[0];
        assert_eq!(fountain.required_string("kind").unwrap(), "fountain");
        assert_eq!(fountain.required_string("name").unwrap(), "Spritz-Brunnen");
        assert_eq!(
            fountain.optional_string("source_id").as_deref(),
            Some("abc")
        );

        let swim = &snapshot.pois.features[1];
        assert_eq!(swim.required_string("kind").unwrap(), "swim_area");
        assert_eq!(swim.optional_string("source_id"), None);

        let reporting = &snapshot.stations.features[0];
        assert_eq!(reporting.required_string("id").unwrap(), "0020F940");
        assert_eq!(
            reporting.property("temperature_c").and_then(Value::as_f64),
            Some(31.4)
        );

        let silent = &snapshot.stations.features[1];
        assert!(silent.property("temperature_c").unwrap().is_null());
    }

    /// A Point's representative point is its own coordinate.
    #[test]
    fn point_representative_point_is_the_coordinate() {
        let feature: Feature = serde_json::from_value(json!({
            "type": "Feature",
            "geometry": {"type": "Point", "coordinates": [7.5886, 47.5596]},
            "properties": {}
        }))
        .unwrap();
        let (lon, lat) = feature.representative_point().unwrap();
        assert_eq!((lon, lat), (7.5886, 47.5596));
    }

    /// A Polygon's representative point is the mean of its outer ring, so
    /// bbox filters roughly place the whole shape.
    #[test]
    fn polygon_representative_point_is_outer_ring_mean() {
        let feature: Feature = serde_json::from_value(json!({
            "type": "Feature",
            "geometry": {"type": "Polygon", "coordinates": [[
                [7.0, 47.0], [8.0, 47.0], [8.0, 48.0], [7.0, 48.0]
            ]]},
            "properties": {}
        }))
        .unwrap();
        let (lon, lat) = feature.representative_point().unwrap();
        assert!((lon - 7.5).abs() < 1e-9, "lon mean, got {lon}");
        assert!((lat - 47.5).abs() < 1e-9, "lat mean, got {lat}");
    }

    /// Broken geometries fail loudly instead of silently caching NaNs.
    #[test]
    fn malformed_geometry_is_rejected() {
        let feature: Feature = serde_json::from_value(json!({
            "type": "Feature",
            "geometry": {"type": "Point", "coordinates": [7.5]},
            "properties": {}
        }))
        .unwrap();
        assert!(feature.representative_point().is_err());

        let feature: Feature = serde_json::from_value(json!({
            "type": "Feature",
            "geometry": {"type": "LineString", "coordinates": [[7.5, 47.5]]},
            "properties": {}
        }))
        .unwrap();
        assert!(feature.representative_point().is_err());
    }
}
