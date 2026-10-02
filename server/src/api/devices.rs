//! `PUT /api/v1/devices/{device_id}` – anonymous device upsert (ADR 0004).
//!
//! The client-generated UUID in the path is the device's whole identity: no
//! accounts, no secrets. The payload is the device's current state – helper
//! flag and, optionally, the live location it shares (null or omitted =
//! not sharing). The first call creates the row; later calls update it and
//! refresh `last_seen_at`. The response is only ever served to the device
//! it describes.

use axum::extract::{Path, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::error::ApiError;
use super::types::LonLat;

/// The device's self-declared state.
#[derive(Debug, Deserialize)]
pub struct DeviceUpsert {
    pub is_helper: bool,
    /// Live location the device shares; null (or omitted) = not sharing.
    #[serde(default)]
    pub location: Option<LonLat>,
}

/// A device, as served back to itself.
#[derive(Debug, Serialize)]
pub struct Device {
    pub device_id: uuid::Uuid,
    pub is_helper: bool,
    pub location: Option<LonLat>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

/// Inserts the device if unknown, otherwise updates its state and refreshes
/// `last_seen_at`. Idempotent per device_id.
pub async fn upsert_device(
    State(pool): State<PgPool>,
    Path(device_id): Path<uuid::Uuid>,
    Json(payload): Json<DeviceUpsert>,
) -> Result<Json<Device>, ApiError> {
    if let Some(location) = &payload.location {
        ApiError::check(location.validate())?;
    }

    // Two statements instead of one so that an omitted location ("not
    // sharing") leaves a previously shared location untouched on update;
    // an explicit null clears it via EXCLUDED.last_location = NULL.
    let (created_at, last_seen_at): (DateTime<Utc>, DateTime<Utc>) = match &payload.location {
        Some(location) => sqlx::query_as(
            "INSERT INTO devices (id, is_helper, last_location, last_seen_at) \
             VALUES ($1, $2, ST_SetSRID(ST_MakePoint($3, $4), 4326)::geography, now()) \
             ON CONFLICT (id) DO UPDATE SET \
                 is_helper = EXCLUDED.is_helper, \
                 last_location = EXCLUDED.last_location, \
                 last_seen_at = now() \
             RETURNING created_at, last_seen_at",
        )
        .bind(device_id)
        .bind(payload.is_helper)
        .bind(location.lon)
        .bind(location.lat)
        .fetch_one(&pool)
        .await?,
        None => sqlx::query_as(
            "INSERT INTO devices (id, is_helper, last_seen_at) \
             VALUES ($1, $2, now()) \
             ON CONFLICT (id) DO UPDATE SET \
                 is_helper = EXCLUDED.is_helper, \
                 last_seen_at = now() \
             RETURNING created_at, last_seen_at",
        )
        .bind(device_id)
        .bind(payload.is_helper)
        .fetch_one(&pool)
        .await?,
    };

    Ok(Json(Device {
        device_id,
        is_helper: payload.is_helper,
        location: payload.location,
        created_at,
        last_seen_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{new_device_id, send_json, test_app};
    use axum::http::StatusCode;
    use chrono::{DateTime, Utc};
    use serde_json::json;

    fn skip() {
        eprintln!("DATABASE_URL not set – skipping postgres test");
    }

    fn parse_rfc3339(value: &serde_json::Value) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value.as_str().expect("RFC 3339 string"))
            .expect("parsable timestamp")
            .with_timezone(&Utc)
    }

    /// The first upsert for a fresh UUID creates the device and echoes its
    /// state (helper flag, shared location, server timestamps).
    #[tokio::test]
    async fn first_upsert_creates_device() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let device_id = new_device_id();

        let (status, body) = send_json(
            app,
            "PUT",
            &format!("/api/v1/devices/{device_id}"),
            Some(json!({
                "is_helper": true,
                "location": {"lon": 7.5886, "lat": 47.5596},
            })),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["device_id"], device_id.to_string());
        assert_eq!(body["is_helper"], true);
        assert_eq!(body["location"]["lon"], 7.5886);
        assert_eq!(body["location"]["lat"], 47.5596);
        parse_rfc3339(&body["created_at"]);
        parse_rfc3339(&body["last_seen_at"]);
    }

    /// A device that omits the location field registers as not sharing.
    #[tokio::test]
    async fn upsert_without_location_shares_none() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let device_id = new_device_id();

        let (status, body) = send_json(
            app,
            "PUT",
            &format!("/api/v1/devices/{device_id}"),
            Some(json!({ "is_helper": false })),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["is_helper"], false);
        assert_eq!(body["location"], serde_json::Value::Null);
    }

    /// A second upsert updates the device's state (helper flag flips,
    /// explicit null clears the shared location), keeps `created_at` and
    /// refreshes `last_seen_at`.
    #[tokio::test]
    async fn second_upsert_updates_state_and_refreshes_last_seen() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let device_id = new_device_id();
        let uri = format!("/api/v1/devices/{device_id}");

        let (_, first) = send_json(
            app.clone(),
            "PUT",
            &uri,
            Some(json!({
                "is_helper": true,
                "location": {"lon": 7.5886, "lat": 47.5596},
            })),
        )
        .await;

        let (status, second) = send_json(
            app,
            "PUT",
            &uri,
            Some(json!({ "is_helper": false, "location": null })),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(second["device_id"], device_id.to_string());
        assert_eq!(second["is_helper"], false);
        assert_eq!(second["location"], serde_json::Value::Null);
        assert_eq!(
            parse_rfc3339(&second["created_at"]),
            parse_rfc3339(&first["created_at"]),
            "created_at must not change on update"
        );
        assert!(
            parse_rfc3339(&second["last_seen_at"]) >= parse_rfc3339(&first["last_seen_at"]),
            "last_seen_at must be refreshed"
        );
    }

    /// Out-of-range coordinates are rejected with 422 and a JSON error body.
    #[tokio::test]
    async fn upsert_rejects_out_of_range_location() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let device_id = new_device_id();

        let (status, body) = send_json(
            app,
            "PUT",
            &format!("/api/v1/devices/{device_id}"),
            Some(json!({
                "is_helper": true,
                "location": {"lon": 200.0, "lat": 47.5596},
            })),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body["error"].as_str().expect("error message").contains("lon"));
    }
}
