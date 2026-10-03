//! `PUT /api/v1/devices/{device_id}` – anonymous device upsert (ADR 0004).
//!
//! The client-generated UUID in the path is the device's whole identity: no
//! accounts, no secrets. The payload is the device's current state – helper
//! flag and, optionally, the live location it shares (null or omitted =
//! not sharing). The first call creates the row; later calls update it and
//! refresh `last_seen_at`. The response is only ever served to the device
//! it describes.
//!
//! A shared location is also forwarded to the live-location API when one is
//! configured (see [`crate::helper_api`]): fire-and-forget after the database
//! write, so the app's PUT neither waits for nor fails because of that
//! service. Not sharing (null location) forwards nothing – the API has no
//! delete; SOS matching filters opted-out devices itself.

use axum::extract::{Path, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::error::{ApiError, ApiJson};
use super::types::LonLat;
use super::AppState;

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
    State(state): State<AppState>,
    Path(device_id): Path<uuid::Uuid>,
    ApiJson(payload): ApiJson<DeviceUpsert>,
) -> Result<Json<Device>, ApiError> {
    if let Some(location) = &payload.location {
        ApiError::check(location.validate())?;
    }

    // Two statements instead of one so the location is written explicitly
    // on both paths: a shared location is stored on insert and kept on
    // conflict via EXCLUDED, while an omitted/null location ("not sharing")
    // clears the stored point on update. Clearing is essential for matching:
    // every upsert refreshes `last_seen_at`, so keeping a stale point alive
    // would keep a helper who stopped sharing matchable for up to 24 h.
    let (created_at, last_seen_at): (DateTime<Utc>, DateTime<Utc>) = match &payload.location {
        Some(location) => {
            sqlx::query_as(
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
            .fetch_one(&state.pool)
            .await?
        }
        None => {
            sqlx::query_as(
                "INSERT INTO devices (id, is_helper, last_seen_at) \
             VALUES ($1, $2, now()) \
             ON CONFLICT (id) DO UPDATE SET \
                 is_helper = EXCLUDED.is_helper, \
                 last_location = NULL, \
                 last_seen_at = now() \
             RETURNING created_at, last_seen_at",
            )
            .bind(device_id)
            .bind(payload.is_helper)
            .fetch_one(&state.pool)
            .await?
        }
    };

    // Fire-and-forget: the database is the source of truth, the helper API is
    // best effort (failures are only logged).
    if let (Some(helper_api), Some(location)) = (state.helper_api.clone(), payload.location) {
        tokio::spawn(async move {
            if let Err(err) = helper_api.post_location(device_id, location).await {
                eprintln!(
                    "helper API: forwarding the location of device {device_id} failed: {err:#}"
                );
            }
        });
    }

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
    use super::super::test_support::{
        new_device_id, router_with_state, send_json, send_raw, skip, test_app, test_state,
        test_state_with_helper_api, MockHelperApi,
    };
    use axum::http::StatusCode;
    use chrono::{DateTime, Utc};
    use serde_json::json;

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

    /// A device that stops sharing – update with `location` omitted – has
    /// its stored point cleared. The devices API has no read endpoint, so
    /// the row is observed through the pool; the alternative (matching)
    /// could not distinguish a cleared point from a stale-but-fresh one,
    /// which is exactly the 24 h ghost the clearing prevents (ADR 0004).
    #[tokio::test]
    async fn update_without_location_clears_stored_location() {
        let Some(state) = test_state().await else {
            skip();
            return;
        };
        let app = router_with_state(&state);
        let device_id = new_device_id();
        let uri = format!("/api/v1/devices/{device_id}");

        let (status, _) = send_json(
            app.clone(),
            "PUT",
            &uri,
            Some(json!({
                "is_helper": true,
                "location": {"lon": 7.5886, "lat": 47.5596},
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // Stop sharing: location omitted from the update.
        let (status, body) =
            send_json(app.clone(), "PUT", &uri, Some(json!({ "is_helper": true }))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["location"], serde_json::Value::Null);
        let (location_cleared,): (bool,) =
            sqlx::query_as("SELECT last_location IS NULL FROM devices WHERE id = $1")
                .bind(device_id)
                .fetch_one(&state.pool)
                .await
                .expect("device row exists");
        assert!(location_cleared, "omitted location must clear the row");

        // Sharing again, then an explicit null: the same clearing applies.
        let (status, _) = send_json(
            app.clone(),
            "PUT",
            &uri,
            Some(json!({
                "is_helper": true,
                "location": {"lon": 7.5886, "lat": 47.5596},
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = send_json(
            app,
            "PUT",
            &uri,
            Some(json!({ "is_helper": true, "location": null })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (location_cleared,): (bool,) =
            sqlx::query_as("SELECT last_location IS NULL FROM devices WHERE id = $1")
                .bind(device_id)
                .fetch_one(&state.pool)
                .await
                .expect("device row exists");
        assert!(location_cleared, "explicit null must clear the row too");
    }

    /// A malformed JSON body is a validation failure in the uniform error
    /// shape (422 `{"error": …}`), not axum's plain-text rejection.
    #[tokio::test]
    async fn upsert_malformed_body_gets_uniform_json_error() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let device_id = new_device_id();

        let (status, bytes) = send_raw(
            app,
            "PUT",
            &format!("/api/v1/devices/{device_id}"),
            "{not json",
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).expect("rejection body is the uniform JSON shape");
        assert!(body["error"].is_string(), "error body explains: {body}");
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
        assert!(body["error"]
            .as_str()
            .expect("error message")
            .contains("lon"));
    }

    /// A PUT with a location forwards exactly one `POST /location` to the
    /// helper API, with the device id as query parameter and the coordinates
    /// as `{longitude, latitude}`.
    #[tokio::test]
    async fn upsert_with_location_forwards_it_to_the_helper_api() {
        let mock = MockHelperApi::spawn().await;
        let Some(state) = test_state_with_helper_api(&mock.url()).await else {
            skip();
            return;
        };
        let app = router_with_state(&state);
        let device_id = new_device_id();

        let (status, _) = send_json(
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

        let calls = mock.wait_for_location_calls(1).await;
        // Let a (wrong) second call show up before asserting "exactly one".
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let calls_after = mock.location_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls_after.len(), 1, "exactly one forwarded call");
        assert_eq!(calls_after[0].device_id, device_id.to_string());
        assert_eq!(
            calls_after[0].body,
            json!({"longitude": 7.5886, "latitude": 47.5596})
        );
    }

    /// A PUT without a shared location (explicit null or omitted) forwards
    /// nothing.
    #[tokio::test]
    async fn upsert_without_location_forwards_nothing() {
        let mock = MockHelperApi::spawn().await;
        let Some(state) = test_state_with_helper_api(&mock.url()).await else {
            skip();
            return;
        };
        let app = router_with_state(&state);
        let uri = format!("/api/v1/devices/{}", new_device_id());

        for body in [
            json!({ "is_helper": false, "location": null }),
            json!({ "is_helper": false }),
        ] {
            let (status, _) = send_json(app.clone(), "PUT", &uri, Some(body)).await;
            assert_eq!(status, StatusCode::OK);
        }

        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        assert!(mock.location_calls().is_empty(), "nothing is forwarded");
    }

    /// The PUT still succeeds when the helper API answers 500 (the call is
    /// attempted, its failure is only logged).
    #[tokio::test]
    async fn upsert_succeeds_when_the_helper_api_fails() {
        let mock = MockHelperApi::spawn().await;
        mock.set_failing(true);
        let Some(state) = test_state_with_helper_api(&mock.url()).await else {
            skip();
            return;
        };
        let app = router_with_state(&state);

        let (status, body) = send_json(
            app,
            "PUT",
            &format!("/api/v1/devices/{}", new_device_id()),
            Some(json!({
                "is_helper": true,
                "location": {"lon": 7.5886, "lat": 47.5596},
            })),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "PUT must not fail: {body}");
        assert_eq!(mock.wait_for_location_calls(1).await.len(), 1);
    }

    /// The PUT still succeeds, promptly, when the helper API is unreachable.
    #[tokio::test]
    async fn upsert_succeeds_when_the_helper_api_is_unreachable() {
        let Some(state) = test_state_with_helper_api(&MockHelperApi::unreachable_url()).await
        else {
            skip();
            return;
        };
        let app = router_with_state(&state);

        let (status, body) = send_json(
            app,
            "PUT",
            &format!("/api/v1/devices/{}", new_device_id()),
            Some(json!({
                "is_helper": true,
                "location": {"lon": 7.5886, "lat": 47.5596},
            })),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "PUT must not fail: {body}");
    }
}
