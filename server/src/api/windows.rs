//! Helper-window CRUD, scoped to the calling device (ADR 0004).
//!
//! A window is recurring weekly availability: `weekday` 0=Monday..6=Sunday,
//! `start_time`/`end_time` as Europe/Zurich local wall times, a location
//! point with its own radius in meters, a label, and the `active` vacation
//! toggle (an inactive window stays stored but never matches – ticket 05).
//! Every query filters on the `device_id` from the path, so a device can
//! never see or touch another device's windows; unknown and foreign windows
//! are both reported as 404.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::NaiveTime;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::error::ApiError;
use super::types::{self, wall_time, LonLat};

/// Create payload (POST `/devices/{id}/windows`). `active` defaults to true.
#[derive(Debug, Deserialize)]
pub struct WindowCreate {
    pub weekday: i16,
    #[serde(with = "wall_time")]
    pub start_time: NaiveTime,
    #[serde(with = "wall_time")]
    pub end_time: NaiveTime,
    pub location: LonLat,
    pub radius_m: f64,
    pub label: String,
    #[serde(default = "default_active")]
    pub active: bool,
}

fn default_active() -> bool {
    true
}

/// Partial update payload (PATCH `/devices/{id}/windows/{wid}`): any subset
/// of fields; absent fields keep their stored value.
#[derive(Debug, Default, Deserialize)]
pub struct WindowPatch {
    pub weekday: Option<i16>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub location: Option<LonLat>,
    pub radius_m: Option<f64>,
    pub label: Option<String>,
    pub active: Option<bool>,
}

/// One availability window, as served to the owning device.
#[derive(Debug, Serialize)]
pub struct Window {
    pub id: i64,
    pub device_id: uuid::Uuid,
    pub active: bool,
    pub weekday: i16,
    #[serde(with = "wall_time")]
    pub start_time: NaiveTime,
    #[serde(with = "wall_time")]
    pub end_time: NaiveTime,
    pub location: LonLat,
    pub radius_m: f64,
    pub label: String,
}

/// Column order shared by SELECT and RETURNING clauses.
type WindowRow = (i64, bool, i16, NaiveTime, NaiveTime, f64, f64, f64, String);

impl Window {
    fn from_row(device_id: uuid::Uuid, row: WindowRow) -> Self {
        let (id, active, weekday, start_time, end_time, lon, lat, radius_m, label) = row;
        Self {
            id,
            device_id,
            active,
            weekday,
            start_time,
            end_time,
            location: LonLat { lon, lat },
            radius_m,
            label,
        }
    }
}

/// True when the error is the helper_windows → devices foreign-key
/// violation (i.e. the path's device never registered).
fn is_device_fk_violation(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::Database(db)
            if db.constraint() == Some("helper_windows_device_id_fkey")
    )
}

/// Validates a full window state: weekday 0–6, end strictly after start,
/// WGS84 location, radius > 0.
fn validate_window(
    weekday: i16,
    start_time: NaiveTime,
    end_time: NaiveTime,
    location: LonLat,
    radius_m: f64,
) -> Result<(), ApiError> {
    ApiError::check(types::validate_weekday(weekday))?;
    ApiError::check(types::validate_time_range(start_time, end_time))?;
    ApiError::check(location.validate())?;
    ApiError::check(types::validate_radius_m(radius_m))?;
    Ok(())
}

pub async fn create_window(
    State(pool): State<PgPool>,
    Path(device_id): Path<uuid::Uuid>,
    Json(payload): Json<WindowCreate>,
) -> Result<(StatusCode, Json<Window>), ApiError> {
    validate_window(
        payload.weekday,
        payload.start_time,
        payload.end_time,
        payload.location,
        payload.radius_m,
    )?;

    const INSERT: &str = "INSERT INTO helper_windows \
         (device_id, active, weekday, start_time, end_time, location, radius_m, label) \
         VALUES ($1, $2, $3, $4, $5, ST_SetSRID(ST_MakePoint($6, $7), 4326)::geography, $8, $9) \
         RETURNING id, active, weekday, start_time, end_time, \
             ST_X(location::geometry), ST_Y(location::geometry), radius_m, label";
    let row: WindowRow = sqlx::query_as(INSERT)
        .bind(device_id)
        .bind(payload.active)
        .bind(payload.weekday)
        .bind(payload.start_time)
        .bind(payload.end_time)
        .bind(payload.location.lon)
        .bind(payload.location.lat)
        .bind(payload.radius_m)
        .bind(&payload.label)
        .fetch_one(&pool)
        .await
        .map_err(|err| {
            // Windows hang off a registered device; a missing device_id is a
            // client error (404), not a server fault.
            if is_device_fk_violation(&err) {
                ApiError::NotFound("device not found")
            } else {
                err.into()
            }
        })?;

    Ok((StatusCode::CREATED, Json(Window::from_row(device_id, row))))
}

pub async fn list_windows(
    State(pool): State<PgPool>,
    Path(device_id): Path<uuid::Uuid>,
) -> Result<Json<Vec<Window>>, ApiError> {
    const LIST: &str = "SELECT id, active, weekday, start_time, end_time, \
         ST_X(location::geometry), ST_Y(location::geometry), radius_m, label \
         FROM helper_windows WHERE device_id = $1 ORDER BY id";
    let rows: Vec<WindowRow> = sqlx::query_as(LIST)
        .bind(device_id)
        .fetch_all(&pool)
        .await?;

    Ok(Json(
        rows.into_iter()
            .map(|row| Window::from_row(device_id, row))
            .collect(),
    ))
}

pub async fn patch_window(
    State(pool): State<PgPool>,
    Path((device_id, window_id)): Path<(uuid::Uuid, i64)>,
    Json(patch): Json<WindowPatch>,
) -> Result<Json<Window>, ApiError> {
    // Load the owned row first: unknown and foreign windows are the same
    // 404, and the merged state is what gets validated.
    const OWNED: &str = "SELECT id, active, weekday, start_time, end_time, \
         ST_X(location::geometry), ST_Y(location::geometry), radius_m, label \
         FROM helper_windows WHERE id = $1 AND device_id = $2";
    let current: WindowRow = sqlx::query_as(OWNED)
        .bind(window_id)
        .bind(device_id)
        .fetch_optional(&pool)
        .await?
        .ok_or(ApiError::NotFound("window not found"))?;
    let current = Window::from_row(device_id, current);

    let merged_start = match &patch.start_time {
        Some(raw) => types::parse_wall_time(raw).map_err(ApiError::Validation)?,
        None => current.start_time,
    };
    let merged_end = match &patch.end_time {
        Some(raw) => types::parse_wall_time(raw).map_err(ApiError::Validation)?,
        None => current.end_time,
    };
    let merged_location = patch.location.unwrap_or(current.location);
    let merged_radius_m = patch.radius_m.unwrap_or(current.radius_m);
    let merged_weekday = patch.weekday.unwrap_or(current.weekday);
    let merged_label = patch.label.unwrap_or(current.label);
    let merged_active = patch.active.unwrap_or(current.active);

    validate_window(
        merged_weekday,
        merged_start,
        merged_end,
        merged_location,
        merged_radius_m,
    )?;

    const UPDATE: &str = "UPDATE helper_windows SET \
         active = $3, weekday = $4, start_time = $5, end_time = $6, \
         location = ST_SetSRID(ST_MakePoint($7, $8), 4326)::geography, \
         radius_m = $9, label = $10 \
         WHERE id = $1 AND device_id = $2 \
         RETURNING id, active, weekday, start_time, end_time, \
             ST_X(location::geometry), ST_Y(location::geometry), radius_m, label";
    let row: WindowRow = sqlx::query_as(UPDATE)
        .bind(window_id)
        .bind(device_id)
        .bind(merged_active)
        .bind(merged_weekday)
        .bind(merged_start)
        .bind(merged_end)
        .bind(merged_location.lon)
        .bind(merged_location.lat)
        .bind(merged_radius_m)
        .bind(&merged_label)
        .fetch_optional(&pool)
        .await?
        .ok_or(ApiError::NotFound("window not found"))?;

    Ok(Json(Window::from_row(device_id, row)))
}

pub async fn delete_window(
    State(pool): State<PgPool>,
    Path((device_id, window_id)): Path<(uuid::Uuid, i64)>,
) -> Result<StatusCode, ApiError> {
    const DELETE: &str = "DELETE FROM helper_windows WHERE id = $1 AND device_id = $2";
    let result = sqlx::query(DELETE)
        .bind(window_id)
        .bind(device_id)
        .execute(&pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound("window not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{new_device_id, send_json, skip, test_app};
    use axum::http::StatusCode;
    use serde_json::{json, Value};

    /// Registers a device and returns its id plus a valid window payload.
    async fn registered_device_with_payload(app: &axum::Router) -> (uuid::Uuid, Value) {
        let device_id = new_device_id();
        let (status, _) = send_json(
            app.clone(),
            "PUT",
            &format!("/api/v1/devices/{device_id}"),
            Some(json!({ "is_helper": true })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "device registration");
        let payload = json!({
            "weekday": 0,
            "start_time": "09:00",
            "end_time": "17:00",
            "location": {"lon": 7.5886, "lat": 47.5596},
            "radius_m": 500,
            "label": "Büro",
        });
        (device_id, payload)
    }

    async fn list_windows(app: &axum::Router, device_id: uuid::Uuid) -> (StatusCode, Value) {
        send_json(
            app.clone(),
            "GET",
            &format!("/api/v1/devices/{device_id}/windows"),
            None,
        )
        .await
    }

    /// A created window comes back with an id, its normalized state and the
    /// owning device; the owning device then finds it in its list.
    #[tokio::test]
    async fn created_window_is_listed_for_its_device() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let (device_id, payload) = registered_device_with_payload(&app).await;

        let (status, body) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/devices/{device_id}/windows"),
            Some(payload),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["device_id"], device_id.to_string());
        assert!(body["id"].as_i64().expect("window id") > 0);
        assert_eq!(body["active"], true, "windows default to active");
        assert_eq!(body["weekday"], 0);
        assert_eq!(body["start_time"], "09:00:00");
        assert_eq!(body["end_time"], "17:00:00");
        assert_eq!(body["location"]["lon"], 7.5886);
        assert_eq!(body["location"]["lat"], 47.5596);
        assert_eq!(body["radius_m"], 500.0);
        assert_eq!(body["label"], "Büro");

        let (status, listed) = list_windows(&app, device_id).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(listed.as_array().expect("array").len(), 1);
        assert_eq!(listed[0]["id"], body["id"]);
    }

    /// Invalid window state is rejected with 422 and nothing is stored.
    #[tokio::test]
    async fn create_rejects_invalid_windows() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let (device_id, valid) = registered_device_with_payload(&app).await;
        let uri = format!("/api/v1/devices/{device_id}/windows");

        let invalid_payloads = vec![
            json!({"weekday": 7}),
            json!({"weekday": -1}),
            json!({"radius_m": 0}),
            json!({"radius_m": -5}),
            json!({"end_time": "09:00"}), // end == start
            json!({"end_time": "08:00"}), // end < start
            json!({"location": {"lon": 200.0, "lat": 47.5}}),
            json!({"location": {"lon": 7.5, "lat": 91.0}}),
        ];
        for override_payload in invalid_payloads {
            let mut payload = valid.clone();
            payload.as_object_mut().unwrap().extend(
                override_payload
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
            let (status, body) = send_json(app.clone(), "POST", &uri, Some(payload)).await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "expected 422 for {override_payload}"
            );
            assert!(body["error"].is_string(), "error body explains: {body}");
        }

        let (_, listed) = list_windows(&app, device_id).await;
        assert_eq!(listed.as_array().expect("array").len(), 0, "nothing stored");
    }

    /// Windows cannot hang off a device that never registered.
    #[tokio::test]
    async fn create_for_unknown_device_is_404() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let unknown = new_device_id();

        let (status, _) = send_json(
            app,
            "POST",
            &format!("/api/v1/devices/{unknown}/windows"),
            Some(json!({
                "weekday": 0,
                "start_time": "09:00",
                "end_time": "17:00",
                "location": {"lon": 7.5886, "lat": 47.5596},
                "radius_m": 500,
                "label": "nowhere",
            })),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    async fn create_window_for(app: &axum::Router, device_id: uuid::Uuid, payload: Value) -> Value {
        let (status, body) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/devices/{device_id}/windows"),
            Some(payload),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "window creation: {body}");
        body
    }

    /// PATCH updates single fields (here: the `active` vacation switch)
    /// without deleting the window.
    #[tokio::test]
    async fn patch_toggles_active_without_deleting() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let (device_id, payload) = registered_device_with_payload(&app).await;
        let window = create_window_for(&app, device_id, payload).await;
        let uri = format!("/api/v1/devices/{device_id}/windows/{}", window["id"]);

        // Vacation: toggle off. The window must stay listed, inactive.
        let (status, patched) =
            send_json(app.clone(), "PATCH", &uri, Some(json!({"active": false}))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(patched["active"], false);
        assert_eq!(patched["label"], "Büro", "unrelated fields stay");

        let (_, listed) = list_windows(&app, device_id).await;
        assert_eq!(listed.as_array().expect("array").len(), 1, "still stored");
        assert_eq!(listed[0]["active"], false);

        // Back from holiday: toggle on again.
        let (status, patched) = send_json(app, "PATCH", &uri, Some(json!({"active": true}))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(patched["active"], true);
    }

    /// PATCH accepts several fields at once and normalizes times.
    #[tokio::test]
    async fn patch_updates_multiple_fields() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let (device_id, payload) = registered_device_with_payload(&app).await;
        let window = create_window_for(&app, device_id, payload).await;
        let uri = format!("/api/v1/devices/{device_id}/windows/{}", window["id"]);

        let (status, patched) = send_json(
            app,
            "PATCH",
            &uri,
            Some(json!({
                "weekday": 5,
                "start_time": "08:15",
                "end_time": "12:30:00",
                "label": "Rhein",
                "radius_m": 250.5,
                "location": {"lon": 7.5944, "lat": 47.5667},
            })),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(patched["weekday"], 5);
        assert_eq!(patched["start_time"], "08:15:00");
        assert_eq!(patched["end_time"], "12:30:00");
        assert_eq!(patched["label"], "Rhein");
        assert_eq!(patched["radius_m"], 250.5);
        assert_eq!(patched["location"]["lon"], 7.5944);
        assert_eq!(patched["active"], true);
    }

    /// PATCH revalidates the *merged* state: changing one field can break a
    /// rule against the stored others, and the window stays unchanged.
    #[tokio::test]
    async fn patch_revalidates_merged_state() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let (device_id, payload) = registered_device_with_payload(&app).await;
        let window = create_window_for(&app, device_id, payload).await;
        let uri = format!("/api/v1/devices/{device_id}/windows/{}", window["id"]);

        let rejects = vec![
            json!({"end_time": "08:00"}),   // before stored start
            json!({"start_time": "18:00"}), // after stored end
            json!({"weekday": 7}),
            json!({"radius_m": 0}),
            json!({"location": {"lon": 999.0, "lat": 0.0}}),
            json!({"start_time": "nope"}),
        ];
        for reject in rejects {
            let (status, body) = send_json(app.clone(), "PATCH", &uri, Some(reject.clone())).await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "expected 422 for {reject}"
            );
            assert!(body["error"].is_string());
        }

        let (_, listed) = list_windows(&app, device_id).await;
        assert_eq!(listed[0]["start_time"], "09:00:00", "untouched");
        assert_eq!(listed[0]["end_time"], "17:00:00", "untouched");
        assert_eq!(listed[0]["weekday"], 0, "untouched");
    }

    /// DELETE removes the window; deleting it again is 404.
    #[tokio::test]
    async fn delete_removes_window_then_404s() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let (device_id, payload) = registered_device_with_payload(&app).await;
        let window = create_window_for(&app, device_id, payload).await;
        let uri = format!("/api/v1/devices/{device_id}/windows/{}", window["id"]);

        let (status, body) = send_json(app.clone(), "DELETE", &uri, None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(body, Value::Null);

        let (_, listed) = list_windows(&app, device_id).await;
        assert_eq!(listed.as_array().expect("array").len(), 0);

        let (status, _) = send_json(app, "DELETE", &uri, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// Anonymity guarantee: another device's windows are invisible and
    /// untouchable – its list is empty, and PATCH/DELETE on a foreign
    /// window return 404 without changing it.
    #[tokio::test]
    async fn other_devices_windows_are_invisible_and_untouchable() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let (owner_id, payload) = registered_device_with_payload(&app).await;
        let window = create_window_for(&app, owner_id, payload).await;

        let intruder = new_device_id();
        let (status, _) = send_json(
            app.clone(),
            "PUT",
            &format!("/api/v1/devices/{intruder}"),
            Some(json!({"is_helper": true})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // The intruder knows (or guesses) the window id and addresses it
        // under *their own* device identity.
        let foreign_uri = format!("/api/v1/devices/{intruder}/windows/{}", window["id"]);

        let (status, listed) = list_windows(&app, intruder).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            listed.as_array().expect("array").len(),
            0,
            "foreign windows invisible"
        );

        let (status, _) = send_json(
            app.clone(),
            "PATCH",
            &foreign_uri,
            Some(json!({"active": false})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "foreign PATCH rejected");

        let (status, _) = send_json(app.clone(), "DELETE", &foreign_uri, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "foreign DELETE rejected");

        // The owner's window survived both attempts untouched.
        let (_, listed) = list_windows(&app, owner_id).await;
        assert_eq!(listed[0]["active"], true, "foreign PATCH must not mutate");
    }
}
