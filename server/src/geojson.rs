//! Minimal GeoJSON (RFC 7946) types.
//!
//! Coordinate order is `[lon, lat]` everywhere: longitude first, latitude
//! second. `Position` is a newtype over an ordered pair so that the order is
//! fixed at the type level and cannot be flipped accidentally.

use serde::Serialize;

/// A GeoJSON position: `[lon, lat]` (longitude first, per RFC 7946 §3.1.1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Position(pub f64, pub f64);

impl Position {
    /// Unused until POIs flow in from the database (ticket 02); kept here so
    /// the `[lon, lat]` contract has a single home.
    #[allow(dead_code)]
    pub fn new(lon: f64, lat: f64) -> Self {
        Self(lon, lat)
    }
}

/// A GeoJSON geometry. Only the Point variant is needed for now.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum Geometry {
    Point { coordinates: Position },
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
    #[allow(dead_code)] // used once POIs flow in from the database (ticket 02)
    pub fn point(
        position: Position,
        properties: serde_json::Map<String, serde_json::Value>,
    ) -> Self {
        Self {
            kind: "Feature",
            geometry: Geometry::Point {
                coordinates: position,
            },
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
        let json = serde_json::to_value(Position::new(7.5886, 47.5596)).unwrap();
        assert_eq!(json, serde_json::json!([7.5886, 47.5596]));
    }

    /// A point feature carries `type: "Feature"`, a Point geometry and its
    /// properties.
    #[test]
    fn point_feature_serializes_as_geojson() {
        let feature = Feature::point(Position::new(7.5886, 47.5596), Default::default());
        let json = serde_json::to_value(feature).unwrap();
        assert_eq!(json["type"], "Feature");
        assert_eq!(json["geometry"]["type"], "Point");
        assert_eq!(
            json["geometry"]["coordinates"],
            serde_json::json!([7.5886, 47.5596])
        );
        assert_eq!(json["properties"], serde_json::json!({}));
    }
}
