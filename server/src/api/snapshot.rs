//! `GET /api/v1/snapshot` – full offline-sync snapshot.
//!
//! The snapshot is the app's whole world: every POI and every station with
//! its latest measurement, replaced wholesale into the on-device cache.
//! Ticket 01 serves a valid but *empty* snapshot; later tickets fill it from
//! Postgres.

use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::geojson::FeatureCollection;

/// The full-snapshot response document.
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub pois: FeatureCollection,
    pub stations: FeatureCollection,
    pub generated_at: DateTime<Utc>,
}

impl Snapshot {
    pub fn empty() -> Self {
        Self {
            pois: FeatureCollection::default(),
            stations: FeatureCollection::default(),
            generated_at: Utc::now(),
        }
    }
}

pub async fn get_snapshot() -> Json<Snapshot> {
    Json(Snapshot::empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The empty snapshot serializes with GeoJSON FeatureCollections and a
    /// UTC `generated_at` stamp.
    #[test]
    fn empty_snapshot_serializes_with_feature_collections() {
        let json = serde_json::to_value(Snapshot::empty()).unwrap();
        assert_eq!(json["pois"]["type"], "FeatureCollection");
        assert_eq!(json["pois"]["features"], serde_json::json!([]));
        assert_eq!(json["stations"]["type"], "FeatureCollection");
        assert_eq!(json["stations"]["features"], serde_json::json!([]));
        DateTime::parse_from_rfc3339(json["generated_at"].as_str().unwrap()).unwrap();
    }
}
