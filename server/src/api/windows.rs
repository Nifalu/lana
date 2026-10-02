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

const WINDOW_COLUMNS: &str = "id, active, weekday, start_time, end_time, \
     ST_X(location::geometry), ST_Y(location::geometry), radius_m, label";

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
    Err(ApiError::NotFound("not implemented"))
}

pub async fn delete_window(
    State(pool): State<PgPool>,
    Path((device_id, window_id)): Path<(uuid::Uuid, i64)>,
) -> Result<StatusCode, ApiError> {
    Err(ApiError::NotFound("not implemented"))
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
            json!({"end_time": "09:00"}),  // end == start
            json!({"end_time": "08:00"}),  // end < start
            json!({"location": {"lon": 200.0, "lat": 47.5}}),
            json!({"location": {"lon": 7.5, "lat": 91.0}}),
        ];
        for override_payload in invalid_payloads {
            let mut payload = valid.clone();
            payload.as_object_mut().unwrap().extend(
                override_payload.as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())),
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
}
