//! Minimal GeoJSON (RFC 7946) types.
//!
//! Coordinate order is `[lon, lat]` everywhere: longitude first, latitude
//! second. `Position` is a newtype over an ordered pair so that the order is
//! fixed at the type level and cannot be flipped accidentally.

use serde::{Deserialize, Serialize};

/// A GeoJSON position: `[lon, lat]` (longitude first, per RFC 7946 §3.1.1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position(pub f64, pub f64);

/// A GeoJSON geometry. Points cover fountains, cool places and stations;
/// Polygons preserve the Rhine swim-area shapes for map drawing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Geometry {
    Point { coordinates: Position },
    Polygon { coordinates: Vec<Vec<Position>> },
}

/// A GeoJSON Feature: a geometry plus arbitrary JSON properties.
#[derive(Debug, Clone, Serialize)]
pub struct Feature {
    #[serde(rename = "type")]
    kind: &'static str,
    geometry: Geometry,
    properties: serde_json::Map<String, serde_json::Value>,
}

impl Feature {
    /// Builds a feature from any supported geometry plus its properties.
    pub fn new(geometry: Geometry, properties: serde_json::Map<String, serde_json::Value>) -> Self {
        Self {
            kind: "Feature",
            geometry,
            properties,
        }
    }
}

/// A GeoJSON FeatureCollection.
#[derive(Debug, Clone, Serialize)]
pub struct FeatureCollection {
    #[serde(rename = "type")]
    kind: &'static str,
    features: Vec<Feature>,
}

impl Default for FeatureCollection {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl FeatureCollection {
    pub fn new(features: Vec<Feature>) -> Self {
        Self {
            kind: "FeatureCollection",
            features,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Positions always serialize as `[lon, lat]` – longitude first.
    #[test]
    fn position_serializes_lon_before_lat() {
        let json = serde_json::to_value(Position(7.5886, 47.5596)).unwrap();
        assert_eq!(json, serde_json::json!([7.5886, 47.5596]));
    }

    /// A point feature carries `type: "Feature"`, a Point geometry and its
    /// properties.
    #[test]
    fn point_feature_serializes_as_geojson() {
        let feature = Feature::new(
            Geometry::Point {
                coordinates: Position(7.5886, 47.5596),
            },
            Default::default(),
        );
        let json = serde_json::to_value(feature).unwrap();
        assert_eq!(json["type"], "Feature");
        assert_eq!(json["geometry"]["type"], "Point");
        assert_eq!(
            json["geometry"]["coordinates"],
            serde_json::json!([7.5886, 47.5596])
        );
        assert_eq!(json["properties"], serde_json::json!({}));
    }

    /// A polygon geometry serializes with nested coordinate rings and survives
    /// a serde round-trip (swim-area shapes are stored and served as-is).
    #[test]
    fn polygon_geometry_round_trips() {
        let geometry = Geometry::Polygon {
            coordinates: vec![vec![
                Position(7.5, 47.5),
                Position(7.6, 47.5),
                Position(7.6, 47.6),
                Position(7.5, 47.5),
            ]],
        };
        let json = serde_json::to_value(&geometry).unwrap();
        assert_eq!(json["type"], "Polygon");
        assert_eq!(
            json["coordinates"],
            serde_json::json!([[[7.5, 47.5], [7.6, 47.5], [7.6, 47.6], [7.5, 47.5]]])
        );
        let round_tripped: Geometry = serde_json::from_value(json).unwrap();
        assert_eq!(round_tripped, geometry);
    }

    /// `ST_AsGeoJSON` output (as stored in PostGIS) deserializes back into a
    /// geometry.
    #[test]
    fn deserializes_postgis_st_asgeojson_output() {
        let raw = serde_json::json!({"type":"Point","coordinates":[7.604586, 47.56656]});
        let geometry: Geometry = serde_json::from_value(raw).unwrap();
        assert_eq!(
            geometry,
            Geometry::Point {
                coordinates: Position(7.604586, 47.56656)
            }
        );
    }
}
