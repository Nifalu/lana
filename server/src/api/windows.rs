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
    Err(ApiError::NotFound("not implemented"))
}

pub async fn list_windows(
    State(pool): State<PgPool>,
    Path(device_id): Path<uuid::Uuid>,
) -> Result<Json<Vec<Window>>, ApiError> {
    Err(ApiError::NotFound("not implemented"))
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
