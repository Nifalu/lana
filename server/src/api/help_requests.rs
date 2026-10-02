//! SOS lifecycle: create, list, respond, resolve, cancel (spec ticket 05,
//! ADR 0004).
//!
//! Slice 1 skeleton: only the wire shapes are fixed; the handlers still
//! return 404 so their tests can go red before the implementation.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::error::{ApiError, ApiJson};
use super::types::{self, LonLat};
use super::AppState;

/// Maximum note length in characters (short note, e.g. "dizzy, need water").
pub const MAX_NOTE_CHARS: usize = 500;

/// Default matching radius in meters when the request overrides nothing.
pub const DEFAULT_RADIUS_M: f64 = 500.0;

/// The lifecycle statuses of a help request, in state-machine order.
pub const STATUSES: [&str; 4] = ["open", "responded", "resolved", "cancelled"];

/// Create payload (POST `/help-requests`).
#[derive(Debug, Deserialize)]
pub struct HelpRequestCreate {
    /// The requesting device; auto-registered when unknown (ADR 0004).
    pub device_id: uuid::Uuid,
    /// Where help is needed: a GPS fix or a dropped pin, `[lon, lat]`.
    pub location: LonLat,
    /// Optional short note to helpers.
    #[serde(default)]
    pub note: Option<String>,
    /// Matching radius override in meters (default [`DEFAULT_RADIUS_M`]).
    #[serde(default)]
    pub radius_m: Option<f64>,
}

/// List filters (GET `/help-requests`).
#[derive(Debug, Deserialize)]
pub struct HelpRequestListQuery {
    /// One of `open|responded|resolved|cancelled`.
    pub status: Option<String>,
    /// `lon,lat` – keep requests within `radius_m` of this point.
    pub near: Option<String>,
    /// Radius for the `near` filter in meters (default [`DEFAULT_RADIUS_M`]).
    pub radius_m: Option<f64>,
}

/// A help request as served on the wire.
///
/// Anonymity (ADR 0004): the shape carries NO requester or responder fields –
/// only the opaque request `id` and its status transitions are public.
#[derive(Debug, Serialize)]
pub struct HelpRequest {
    pub id: uuid::Uuid,
    pub status: String,
    pub note: Option<String>,
    pub location: LonLat,
    pub radius_m: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Column order of the public shape in SELECT/RETURNING clauses.
type HelpRequestRow = (
    uuid::Uuid,
    String,
    Option<String>,
    f64,
    f64,
    f64,
    DateTime<Utc>,
    DateTime<Utc>,
);

impl HelpRequest {
    fn from_row(row: HelpRequestRow) -> Self {
        let (id, status, note, lon, lat, radius_m, created_at, updated_at) = row;
        Self {
            id,
            status,
            note,
            location: LonLat { lon, lat },
            radius_m,
            created_at,
            updated_at,
        }
    }
}

fn validate_create(payload: &HelpRequestCreate) -> Result<(), ApiError> {
    ApiError::check(payload.location.validate())?;
    if let Some(radius_m) = payload.radius_m {
        ApiError::check(types::validate_radius_m(radius_m))?;
    }
    if let Some(note) = &payload.note {
        if note.chars().count() > MAX_NOTE_CHARS {
            return Err(ApiError::Validation(format!(
                "note must be at most {MAX_NOTE_CHARS} characters"
            )));
        }
    }
    Ok(())
}

pub async fn create_help_request(
    State(state): State<AppState>,
    ApiJson(payload): ApiJson<HelpRequestCreate>,
) -> Result<(StatusCode, Json<HelpRequest>), ApiError> {
    validate_create(&payload)?;
    let radius_m = payload.radius_m.unwrap_or(DEFAULT_RADIUS_M);
    let request_id = uuid::Uuid::new_v4();

    let mut tx = state.pool.begin().await?;

    // The requester is auto-registered when unknown (ADR 0004): an insert
    // that only bumps `last_seen_at` on conflict, so an existing device
    // keeps its helper flag and shared location.
    sqlx::query(
        "INSERT INTO devices (id, is_helper, last_seen_at) VALUES ($1, FALSE, now()) \
         ON CONFLICT (id) DO UPDATE SET last_seen_at = now()",
    )
    .bind(payload.device_id)
    .execute(&mut *tx)
    .await?;

    const INSERT: &str = "INSERT INTO help_requests \
         (id, requester_id, status, note, location, radius_m) \
         VALUES ($1, $2, 'open', $3, ST_SetSRID(ST_MakePoint($4, $5), 4326)::geography, $6) \
         RETURNING id, status, note, ST_X(location::geometry), ST_Y(location::geometry), \
             radius_m, created_at, updated_at";
    let row: HelpRequestRow = sqlx::query_as(INSERT)
        .bind(request_id)
        .bind(payload.device_id)
        .bind(&payload.note)
        .bind(payload.location.lon)
        .bind(payload.location.lat)
        .bind(radius_m)
        .fetch_one(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(HelpRequest::from_row(row))))
}

/// Parses a `lon,lat` pair for the `near` filter.
fn parse_near(s: &str) -> Result<LonLat, ApiError> {
    let (lon, lat) = s
        .split_once(',')
        .ok_or_else(|| ApiError::Validation("near must be 'lon,lat'".to_string()))?;
    let point = LonLat {
        lon: lon.trim().parse().map_err(|_| {
            ApiError::Validation("near lon must be a number".to_string())
        })?,
        lat: lat.trim().parse().map_err(|_| {
            ApiError::Validation("near lat must be a number".to_string())
        })?,
    };
    ApiError::check(point.validate())?;
    Ok(point)
}

pub async fn list_help_requests(
    State(state): State<AppState>,
    Query(query): Query<HelpRequestListQuery>,
) -> Result<Json<Vec<HelpRequest>>, ApiError> {
    if let Some(status) = &query.status {
        if !STATUSES.contains(&status.as_str()) {
            return Err(ApiError::Validation(format!(
                "status must be one of {}",
                STATUSES.join("|")
            )));
        }
    }
    let near = match &query.near {
        Some(near) => Some(parse_near(near)?),
        None => {
            if query.radius_m.is_some() {
                return Err(ApiError::Validation(
                    "radius_m requires near".to_string(),
                ));
            }
            None
        }
    };
    let radius_m = query.radius_m.unwrap_or(DEFAULT_RADIUS_M);
    if query.radius_m.is_some() {
        ApiError::check(types::validate_radius_m(radius_m))?;
    }

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, status, note, ST_X(location::geometry), ST_Y(location::geometry), \
             radius_m, created_at, updated_at FROM help_requests",
    );
    if let Some(status) = &query.status {
        qb.push(" WHERE status = ").push_bind(status.clone());
    }
    if let Some(near) = near {
        qb.push(if query.status.is_some() { " AND " } else { " WHERE " })
            .push("ST_DWithin(location, ST_SetSRID(ST_MakePoint(")
            .push_bind(near.lon)
            .push(",")
            .push_bind(near.lat)
            .push("), 4326)::geography, ")
            .push_bind(radius_m)
            .push(")");
    }
    qb.push(" ORDER BY created_at DESC");

    let rows: Vec<HelpRequestRow> = qb.build_query_as().fetch_all(&state.pool).await?;
    Ok(Json(rows.into_iter().map(HelpRequest::from_row).collect()))
}

pub async fn respond_help_request(
    State(_state): State<AppState>,
    Path(_request_id): Path<uuid::Uuid>,
) -> Result<Json<HelpRequest>, ApiError> {
    Err(ApiError::NotFound("not implemented"))
}

pub async fn resolve_help_request(
    State(_state): State<AppState>,
    Path(_request_id): Path<uuid::Uuid>,
) -> Result<Json<HelpRequest>, ApiError> {
    Err(ApiError::NotFound("not implemented"))
}

pub async fn cancel_help_request(
    State(_state): State<AppState>,
    Path(_request_id): Path<uuid::Uuid>,
) -> Result<Json<HelpRequest>, ApiError> {
    Err(ApiError::NotFound("not implemented"))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{assert_anonymous, lock_db, new_device_id, scenario_point, send_json, skip, test_app};
    use axum::http::StatusCode;
    use chrono::DateTime;
    use serde_json::{json, Value};

    /// POSTs a help request and asserts 201; returns the body.
    async fn create_request(app: &axum::Router, payload: Value) -> Value {
        let (status, body) = send_json(app.clone(), "POST", "/api/v1/help-requests", Some(payload)).await;
        assert_eq!(status, StatusCode::CREATED, "create help request: {body}");
        body
    }

    /// A create payload at `point` for `device_id`.
    fn create_payload(device_id: uuid::Uuid, point: super::super::types::LonLat) -> Value {
        json!({
            "device_id": device_id,
            "location": {"lon": point.lon, "lat": point.lat},
        })
    }

    fn parse_rfc3339(value: &Value) -> DateTime<chrono::Utc> {
        DateTime::parse_from_rfc3339(value.as_str().expect("RFC 3339 string"))
            .expect("parsable timestamp")
            .with_timezone(&chrono::Utc)
    }

    /// A created SOS starts `open` with a validated location, echoes the
    /// optional note and carries the default radius; the body is the
    /// anonymous public shape (no requester/responder identity, ADR 0004).
    #[tokio::test]
    async fn create_returns_open_request_with_anonymous_public_shape() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let point = scenario_point();

        let body = create_request(
            &app,
            json!({
                "device_id": new_device_id(),
                "location": {"lon": point.lon, "lat": point.lat},
                "note": "dizzy, need water",
            }),
        )
        .await;

        assert!(!body["id"].as_str().unwrap().is_empty(), "server-assigned id");
        assert_eq!(body["status"], "open");
        assert_eq!(body["note"], "dizzy, need water");
        // PostGIS round-trips coordinates with ULP-level drift.
        let lon = body["location"]["lon"].as_f64().unwrap();
        let lat = body["location"]["lat"].as_f64().unwrap();
        assert!((lon - point.lon).abs() < 1e-9, "lon {lon} ≈ {}", point.lon);
        assert!((lat - point.lat).abs() < 1e-9, "lat {lat} ≈ {}", point.lat);
        assert_eq!(body["radius_m"], 500.0, "default matching radius");
        let created = parse_rfc3339(&body["created_at"]);
        let updated = parse_rfc3339(&body["updated_at"]);
        assert!(updated >= created);
        assert_anonymous(&body);
    }

    /// Invalid locations, radii, device ids and overlong notes are rejected
    /// with 422 and nothing is created.
    #[tokio::test]
    async fn create_validates_location_radius_device_and_note() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let point = scenario_point();
        let device_id = new_device_id();

        let invalid_payloads = vec![
            json!({"device_id": device_id, "location": {"lon": 200.0, "lat": point.lat}}),
            json!({"device_id": device_id, "location": {"lon": point.lon, "lat": 91.0}}),
            json!({"device_id": device_id, "location": {"lon": point.lon}}),
            json!({"device_id": device_id, "location": {"lon": point.lon, "lat": point.lat}, "radius_m": 0}),
            json!({"device_id": device_id, "location": {"lon": point.lon, "lat": point.lat}, "radius_m": -3}),
            json!({"device_id": "not-a-uuid", "location": {"lon": point.lon, "lat": point.lat}}),
            json!({"device_id": device_id, "location": {"lon": point.lon, "lat": point.lat},
                   "note": "x".repeat(super::MAX_NOTE_CHARS + 1)}),
        ];
        for payload in invalid_payloads {
            let (status, body) = send_json(app.clone(), "POST", "/api/v1/help-requests", Some(payload.clone())).await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "expected 422 for {payload}"
            );
            assert!(body["error"].is_string(), "error body explains: {body}");
        }
    }

    /// The requester device is auto-registered: after creating an SOS, the
    /// device exists (observable through the windows API, which 404s for
    /// unknown devices), and its helper state is left untouched.
    #[tokio::test]
    async fn create_auto_registers_unknown_requester() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let device_id = new_device_id();

        create_request(&app, create_payload(device_id, scenario_point())).await;

        // The device now exists: a helper window can hang off it.
        let (status, body) = send_json(
            app,
            "POST",
            &format!("/api/v1/devices/{device_id}/windows"),
            Some(json!({
                "weekday": 0,
                "start_time": "09:00",
                "end_time": "17:00",
                "location": {"lon": 7.5886, "lat": 47.5596},
                "radius_m": 500,
                "label": "after auto-register",
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "requester device must exist: {body}");
    }

    /// A per-request radius override is stored and echoed.
    #[tokio::test]
    async fn create_stores_radius_override() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let _db = lock_db().await;

        let body = create_request(
            &app,
            json!({
                "device_id": new_device_id(),
                "location": {"lon": scenario_point().lon, "lat": scenario_point().lat},
                "radius_m": 1500,
            }),
        )
        .await;

        assert_eq!(body["radius_m"], 1500.0);
        assert_anonymous(&body);
    }

    /// The list endpoint filters by status and by near/radius (helper map).
    #[tokio::test]
    async fn list_filters_by_status_and_near_radius() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let base = scenario_point();
        let step = 0.001; // ~111 m south
        let near_a = super::super::types::LonLat { lon: base.lon, lat: base.lat };
        let near_b = super::super::types::LonLat { lon: base.lon, lat: base.lat - step };
        let far = super::super::types::LonLat { lon: base.lon + 0.2, lat: base.lat };

        let a = create_request(&app, create_payload(new_device_id(), near_a)).await;
        let b = create_request(&app, create_payload(new_device_id(), near_b)).await;
        let _far = create_request(&app, create_payload(new_device_id(), far)).await;

        let uri = format!(
            "/api/v1/help-requests?status=open&near={},{}&radius_m=1000",
            base.lon, base.lat
        );
        let (status, listed) = send_json(app.clone(), "GET", &uri, None).await;
        assert_eq!(status, StatusCode::OK);
        let ids: Vec<&str> = listed
            .as_array()
            .expect("array")
            .iter()
            .map(|r| r["id"].as_str().expect("id"))
            .collect();
        assert!(ids.contains(&a["id"].as_str().unwrap()), "near open A in {ids:?}");
        assert!(ids.contains(&b["id"].as_str().unwrap()), "near open B in {ids:?}");
        assert_eq!(ids.len(), 2, "far request filtered out by radius: {ids:?}");
        for request in listed.as_array().unwrap() {
            assert_anonymous(request);
        }

        // A status that nothing matches proves the status filter applies.
        let uri = format!(
            "/api/v1/help-requests?status=responded&near={},{}&radius_m=1000",
            base.lon, base.lat
        );
        let (status, listed) = send_json(app, "GET", &uri, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(listed.as_array().expect("array").len(), 0);
    }

    /// Invalid filter combinations are rejected: unknown status, malformed
    /// near, non-positive radius, radius without near.
    #[tokio::test]
    async fn list_rejects_invalid_filters() {
        let Some(app) = test_app().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let base = scenario_point();

        let invalid_queries = vec![
            format!("status=closed&near={},{}", base.lon, base.lat),
            format!("status=open&near={},{}&radius_m=0", base.lon, base.lat),
            format!("status=open&radius_m=500"),
            "near=not-a-point".to_string(),
        ];
        for query in invalid_queries {
            let (status, body) = send_json(app.clone(), "GET", &format!("/api/v1/help-requests?{query}"), None).await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "expected 422 for ?{query}: {body}"
            );
        }
    }
}
