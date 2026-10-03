//! SOS lifecycle: create, list, respond, resolve, cancel (spec ticket 05,
//! ADR 0004).
//!
//! A request is created `open`, with matching and the `help_request_new`
//! fan-out running atomically on the insert. The first responder claims it
//! (`responded`); repeating the same responder is idempotent, any other
//! device gets a conflict. It ends as `resolved` (requester or responder,
//! only from `responded`) or `cancelled` (requester only, only while
//! `open`); terminal repeats are idempotent for the participants. Every
//! state change fans `help_request_updated` out to the requester, the
//! responder and the originally notified helpers; all bodies carry the
//! anonymous public shape.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::error::{ApiError, ApiJson};
use super::events::Notification;
use super::types::{self, LonLat};
use super::AppState;

/// Maximum note length in characters (short note, e.g. "dizzy, need water").
pub const MAX_NOTE_CHARS: usize = 500;

/// Default matching radius in meters when the request overrides nothing.
pub const DEFAULT_RADIUS_M: f64 = 500.0;

/// The lifecycle statuses of a help request (`open → responded → resolved`
/// / `open → cancelled`), in state-machine order. The serde spelling is the
/// lowercase wire/DB form (migration 0004's `status` CHECK constraint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Open,
    Responded,
    Resolved,
    Cancelled,
}

impl Status {
    /// All statuses, in state-machine order.
    pub const ALL: [Status; 4] = [
        Status::Open,
        Status::Responded,
        Status::Resolved,
        Status::Cancelled,
    ];

    /// The lowercase wire/DB spelling.
    fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Responded => "responded",
            Status::Resolved => "resolved",
            Status::Cancelled => "cancelled",
        }
    }

    /// Parses a wire/DB status; `None` for anything outside the lifecycle.
    fn parse(raw: &str) -> Option<Self> {
        Status::ALL
            .iter()
            .copied()
            .find(|status| status.as_str() == raw)
    }
}

/// The stored `status` column value as the enum: the CHECK constraint in
/// migration 0004 guarantees one of the four lifecycle spellings.
fn lifecycle_status(raw: &str) -> Status {
    Status::parse(raw).expect("CHECK constraint guarantees a lifecycle status")
}

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
    /// `lon,lat` - keep requests within `radius_m` of this point.
    pub near: Option<String>,
    /// Radius for the `near` filter in meters (default [`DEFAULT_RADIUS_M`]).
    pub radius_m: Option<f64>,
}

/// A help request as served on the wire.
///
/// Anonymity (ADR 0004): the shape carries NO requester or responder fields -
/// only the opaque request `id` and its status transitions are public.
#[derive(Debug, Clone, Serialize)]
pub struct HelpRequest {
    pub id: uuid::Uuid,
    pub status: Status,
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

/// The public column list, in [`HelpRequestRow`] order - shared by every
/// SELECT/RETURNING clause in this module. A macro (not a `const`) so the
/// statements splice it into `&'static str` literals, as sqlx requires.
macro_rules! columns {
    () => {
        "id, status, note, ST_X(location::geometry), ST_Y(location::geometry), \
         radius_m, created_at, updated_at"
    };
}

impl HelpRequest {
    fn from_row(row: HelpRequestRow) -> Self {
        let (id, status, note, lon, lat, radius_m, created_at, updated_at) = row;
        Self {
            id,
            status: lifecycle_status(&status),
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

    const INSERT: &str = concat!(
        "INSERT INTO help_requests \
         (id, requester_id, status, note, location, radius_m) \
         VALUES ($1, $2, 'open', $3, ST_SetSRID(ST_MakePoint($4, $5), 4326)::geography, $6) \
         RETURNING ",
        columns!()
    );
    let row: HelpRequestRow = sqlx::query_as(INSERT)
        .bind(request_id)
        .bind(payload.device_id)
        .bind(&payload.note)
        .bind(payload.location.lon)
        .bind(payload.location.lat)
        .bind(radius_m)
        .fetch_one(&mut *tx)
        .await?;

    // Matching happens atomically with the insert (ADR 0004): every device
    // that matches is recorded as "originally notified" so later fan-outs
    // still reach it after its location or windows changed.
    let matched = match_devices(
        &mut tx,
        request_id,
        payload.device_id,
        payload.location,
        radius_m,
    )
    .await?;

    tx.commit().await?;

    if !matched.is_empty() {
        let request = HelpRequest::from_row(row.clone());
        state
            .hub
            .publish(matched, Notification::HelpRequestNew(request));
    }

    Ok((StatusCode::CREATED, Json(HelpRequest::from_row(row))))
}

/// The devices matched for a new SOS, recorded in `help_request_notified`
/// and returned for the SSE push (ADR 0004 matching rule):
///
/// - live location: `is_helper`, a shared `last_location` within the
///   effective radius, seen within the last 24 hours;
/// - OR an `active` recurring window whose weekday + Zurich time-of-day
///   contains now (start and end inclusive) and whose point is within the
///   effective radius.
///
/// The requester is never matched against their own SOS.
async fn match_devices(
    tx: &mut sqlx::PgConnection,
    request_id: uuid::Uuid,
    requester_id: uuid::Uuid,
    location: LonLat,
    radius_m: f64,
) -> Result<Vec<uuid::Uuid>, ApiError> {
    use chrono::Datelike;
    let zurich_now = chrono::Utc::now().with_timezone(&chrono_tz::Europe::Zurich);
    let weekday = zurich_now.weekday().num_days_from_monday() as i16;
    let time_of_day = zurich_now.time();

    const MATCH: &str = "WITH matched AS ( \
            SELECT d.id FROM devices d \
            WHERE d.is_helper AND d.id <> $1 \
              AND ( \
                    (d.last_location IS NOT NULL \
                     AND d.last_seen_at >= now() - INTERVAL '24 hours' \
                     AND ST_DWithin(d.last_location, \
                         ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography, $4)) \
                 OR EXISTS ( \
                     SELECT 1 FROM helper_windows w \
                     WHERE w.device_id = d.id \
                       AND w.active \
                       AND w.weekday = $5 \
                       AND w.start_time <= $6 AND w.end_time >= $6 \
                       AND ST_DWithin(w.location, \
                           ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography, $4)) \
              ) \
        ), \
        notified AS ( \
            INSERT INTO help_request_notified (help_request_id, device_id) \
            SELECT $7, id FROM matched \
            RETURNING device_id \
        ) \
        SELECT device_id FROM notified";
    let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(MATCH)
        .bind(requester_id)
        .bind(location.lon)
        .bind(location.lat)
        .bind(radius_m)
        .bind(weekday)
        .bind(time_of_day)
        .bind(request_id)
        .fetch_all(tx)
        .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Parses a `lon,lat` pair for the `near` filter.
fn parse_near(s: &str) -> Result<LonLat, ApiError> {
    let (lon, lat) = s
        .split_once(',')
        .ok_or_else(|| ApiError::Validation("near must be 'lon,lat'".to_string()))?;
    let point = LonLat {
        lon: lon
            .trim()
            .parse()
            .map_err(|_| ApiError::Validation("near lon must be a number".to_string()))?,
        lat: lat
            .trim()
            .parse()
            .map_err(|_| ApiError::Validation("near lat must be a number".to_string()))?,
    };
    ApiError::check(point.validate())?;
    Ok(point)
}

pub async fn list_help_requests(
    State(state): State<AppState>,
    Query(query): Query<HelpRequestListQuery>,
) -> Result<Json<Vec<HelpRequest>>, ApiError> {
    // Parsed once, up front: an unknown spelling is a client error with
    // the same message as before (the wire form is unchanged).
    let status = match &query.status {
        Some(raw) => match Status::parse(raw) {
            Some(status) => Some(status),
            None => {
                return Err(ApiError::Validation(format!(
                    "status must be one of {}",
                    Status::ALL.map(Status::as_str).join("|")
                )))
            }
        },
        None => None,
    };
    let near = match &query.near {
        Some(near) => Some(parse_near(near)?),
        None => {
            if query.radius_m.is_some() {
                return Err(ApiError::Validation("radius_m requires near".to_string()));
            }
            None
        }
    };
    let radius_m = query.radius_m.unwrap_or(DEFAULT_RADIUS_M);
    if query.radius_m.is_some() {
        ApiError::check(types::validate_radius_m(radius_m))?;
    }

    let mut qb = sqlx::QueryBuilder::new(concat!("SELECT ", columns!(), " FROM help_requests"));
    if let Some(status) = status {
        qb.push(" WHERE status = ").push_bind(status.as_str());
    }
    if let Some(near) = near {
        qb.push(if status.is_some() { " AND " } else { " WHERE " })
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

/// Action payload for respond/resolve/cancel: the acting device (ADR 0004 -
/// the device UUID is the whole identity).
#[derive(Debug, Deserialize)]
pub struct HelpRequestAction {
    pub device_id: uuid::Uuid,
}

/// Responds to an SOS: the first responder wins and the status becomes
/// `responded`; repeating the same responder's call is idempotent; any other
/// device (or the requester) gets a conflict. Unknown requests are 404.
/// A successful transition fans out `help_request_updated` to the requester,
/// the responder and the originally notified helpers.
pub async fn respond_help_request(
    State(state): State<AppState>,
    Path(request_id): Path<uuid::Uuid>,
    ApiJson(action): ApiJson<HelpRequestAction>,
) -> Result<Json<HelpRequest>, ApiError> {
    let mut tx = state.pool.begin().await?;

    // Parties decide every branch (404 vs idempotent vs conflict).
    let parties: Option<(uuid::Uuid, String, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT requester_id, status, responder_id FROM help_requests WHERE id = $1",
    )
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((requester_id, status, responder_id)) = parties else {
        return Err(ApiError::NotFound("help request not found"));
    };
    let status = lifecycle_status(&status);

    let (request, changed) = if status == Status::Open && requester_id != action.device_id {
        // The atomic claim: a concurrent second responder's UPDATE affects no
        // row, and the None branch turns it into the conflict.
        const CLAIM: &str = concat!(
            "UPDATE help_requests \
             SET responder_id = $1, status = 'responded', updated_at = now() \
             WHERE id = $2 AND status = 'open' \
             RETURNING ",
            columns!()
        );
        let claimed: Option<HelpRequestRow> = sqlx::query_as(CLAIM)
            .bind(action.device_id)
            .bind(request_id)
            .fetch_optional(&mut *tx)
            .await?;
        match claimed {
            Some(row) => (HelpRequest::from_row(row), true),
            None => return Err(ApiError::Conflict("someone else is already responding")),
        }
    } else if status == Status::Responded && responder_id == Some(action.device_id) {
        // Idempotent repeat: no state change, no fan-out.
        let request = fetch_request(&mut *tx, request_id)
            .await?
            .ok_or(ApiError::NotFound("help request not found"))?;
        (request, false)
    } else if requester_id == action.device_id {
        return Err(ApiError::Conflict(
            "the requester cannot respond to their own request",
        ));
    } else {
        return Err(ApiError::Conflict(
            "the request is no longer open for responses",
        ));
    };

    tx.commit().await?;
    if changed {
        fan_out_status_change(&state, &request).await?;
    }
    Ok(Json(request))
}

/// Resolves an SOS: requester or responder only, and only from `responded`
/// (the lifecycle is open → responded → resolved / cancelled). Re-resolving
/// is idempotent for the participants; everyone else gets a conflict.
pub async fn resolve_help_request(
    State(state): State<AppState>,
    Path(request_id): Path<uuid::Uuid>,
    ApiJson(action): ApiJson<HelpRequestAction>,
) -> Result<Json<HelpRequest>, ApiError> {
    transition_request(
        &state,
        request_id,
        action.device_id,
        // 'resolved' → idempotent repeat; otherwise resolve is only valid
        // from 'responded'.
        |status, is_participant| {
            if !is_participant {
                Err("only the requester or the responder can resolve")
            } else if matches!(status, Status::Resolved | Status::Responded) {
                Ok(())
            } else {
                Err("the request cannot be resolved from its current status")
            }
        },
        concat!(
            "UPDATE help_requests SET status = 'resolved', updated_at = now() \
             WHERE id = $1 AND status = 'responded' \
             RETURNING ",
            columns!()
        ),
    )
    .await
}

/// Cancels an SOS: requester only, and only while `open`.
pub async fn cancel_help_request(
    State(state): State<AppState>,
    Path(request_id): Path<uuid::Uuid>,
    ApiJson(action): ApiJson<HelpRequestAction>,
) -> Result<Json<HelpRequest>, ApiError> {
    transition_request(
        &state,
        request_id,
        action.device_id,
        |status, is_participant| {
            if !is_participant {
                Err("only the requester can cancel")
            } else if status == Status::Open {
                Ok(())
            } else {
                Err("only an open request can be cancelled")
            }
        },
        concat!(
            "UPDATE help_requests SET status = 'cancelled', updated_at = now() \
             WHERE id = $1 AND status = 'open' \
             RETURNING ",
            columns!()
        ),
    )
    .await
}

/// Shared transition for resolve/cancel: loads the parties, checks the
/// rules (as `check`), applies `update` (a full `UPDATE … RETURNING <public
/// columns>` literal) atomically when a change is due, commits and fans the
/// new status out. Idempotent repeats change nothing and fan nothing out.
async fn transition_request(
    state: &AppState,
    request_id: uuid::Uuid,
    device_id: uuid::Uuid,
    check: impl Fn(Status, bool) -> Result<(), &'static str>,
    update: &'static str,
) -> Result<Json<HelpRequest>, ApiError> {
    let mut tx = state.pool.begin().await?;

    let parties: Option<(uuid::Uuid, String, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT requester_id, status, responder_id FROM help_requests WHERE id = $1",
    )
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((requester_id, status, responder_id)) = parties else {
        return Err(ApiError::NotFound("help request not found"));
    };
    let status = lifecycle_status(&status);
    let is_participant = requester_id == device_id || responder_id == Some(device_id);

    let (request, changed) = if matches!(status, Status::Open | Status::Responded) {
        check(status, is_participant).map_err(ApiError::Conflict)?;
        let row: HelpRequestRow = sqlx::query_as(update)
            .bind(request_id)
            .fetch_one(&mut *tx)
            .await?;
        (HelpRequest::from_row(row), true)
    } else if status == Status::Resolved && is_participant {
        // Terminal repeat: idempotent, no state change, no fan-out.
        let request = fetch_request(&mut *tx, request_id)
            .await?
            .ok_or(ApiError::NotFound("help request not found"))?;
        (request, false)
    } else {
        return Err(ApiError::Conflict(
            "the request cannot transition from its current status",
        ));
    };

    tx.commit().await?;
    if changed {
        fan_out_status_change(state, &request).await?;
    }
    Ok(Json(request))
}

/// The help request's public document, by id.
async fn fetch_request(
    exec: impl sqlx::PgExecutor<'_>,
    request_id: uuid::Uuid,
) -> Result<Option<HelpRequest>, ApiError> {
    const SELECT: &str = concat!("SELECT ", columns!(), " FROM help_requests WHERE id = $1");
    let row: Option<HelpRequestRow> = sqlx::query_as(SELECT)
        .bind(request_id)
        .fetch_optional(exec)
        .await?;
    Ok(row.map(HelpRequest::from_row))
}

/// Publishes `help_request_updated` to everyone following this request:
/// requester + responder + the originally notified helpers (ADR 0004),
/// deduplicated.
async fn fan_out_status_change(state: &AppState, request: &HelpRequest) -> Result<(), ApiError> {
    let (requester_id, responder_id): (uuid::Uuid, Option<uuid::Uuid>) =
        sqlx::query_as("SELECT requester_id, responder_id FROM help_requests WHERE id = $1")
            .bind(request.id)
            .fetch_one(&state.pool)
            .await?;
    let notified: Vec<(uuid::Uuid,)> =
        sqlx::query_as("SELECT device_id FROM help_request_notified WHERE help_request_id = $1")
            .bind(request.id)
            .fetch_all(&state.pool)
            .await?;

    let mut targets = std::collections::HashSet::new();
    targets.insert(requester_id);
    targets.extend(responder_id);
    targets.extend(notified.into_iter().map(|(id,)| id));
    state
        .hub
        .publish(targets, Notification::HelpRequestUpdated(request.clone()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{
        assert_anonymous, lock_db, new_device_id, offset, router_with_state, scenario_point,
        send_json, skip, spawn_server, test_app, test_state, SseStream,
    };
    use axum::http::StatusCode;
    use chrono::DateTime;
    use serde_json::{json, Value};
    use std::time::Duration;

    /// How long a positive SSE assertion waits for its event.
    const SSE_EVENT_TIMEOUT: Duration = Duration::from_secs(10);
    /// How long a negative SSE assertion waits to confirm NO event arrives.
    const SSE_SILENCE_TIMEOUT: Duration = Duration::from_millis(700);

    /// POSTs a help request and asserts 201; returns the body.
    async fn create_request(app: &axum::Router, payload: Value) -> Value {
        let (status, body) =
            send_json(app.clone(), "POST", "/api/v1/help-requests", Some(payload)).await;
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

    /// Registers a device via the devices API (identity: ADR 0004).
    async fn put_device(
        app: &axum::Router,
        device_id: uuid::Uuid,
        is_helper: bool,
        location: Option<super::super::types::LonLat>,
    ) {
        let mut payload = json!({ "is_helper": is_helper });
        if let Some(point) = location {
            payload["location"] = json!({"lon": point.lon, "lat": point.lat});
        }
        let (status, body) = send_json(
            app.clone(),
            "PUT",
            &format!("/api/v1/devices/{device_id}"),
            Some(payload),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "device upsert: {body}");
    }

    /// Creates a helper window for an already-registered device.
    async fn create_window(
        app: &axum::Router,
        device_id: uuid::Uuid,
        weekday: i16,
        location: super::super::types::LonLat,
        active: bool,
    ) -> Value {
        let (status, body) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/devices/{device_id}/windows"),
            Some(json!({
                "weekday": weekday,
                "start_time": "00:00",
                "end_time": "23:59",
                "location": {"lon": location.lon, "lat": location.lat},
                "radius_m": 500,
                "label": "test window",
                "active": active,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "window create: {body}");
        body
    }

    /// Today's weekday in Europe/Zurich as the API encodes it (0=Monday).
    fn today_in_zurich() -> i16 {
        use chrono::Datelike;
        chrono::Utc::now()
            .with_timezone(&chrono_tz::Europe::Zurich)
            .weekday()
            .num_days_from_monday() as i16
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

        assert!(
            !body["id"].as_str().unwrap().is_empty(),
            "server-assigned id"
        );
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
            let (status, body) = send_json(
                app.clone(),
                "POST",
                "/api/v1/help-requests",
                Some(payload.clone()),
            )
            .await;
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
        assert_eq!(
            status,
            StatusCode::CREATED,
            "requester device must exist: {body}"
        );
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

    /// Existing DB-gated tests use the plain router helper.
    async fn list_app() -> Option<axum::Router> {
        let state = test_state().await?;
        Some(router_with_state(&state))
    }

    /// The list endpoint filters by status and by near/radius (helper map).
    #[tokio::test]
    async fn list_filters_by_status_and_near_radius() {
        let Some(app) = list_app().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let base = scenario_point();
        // Small radius and short offsets: this test shares the per-ticket
        // database with every other run, and only a query circle this tight
        // makes cross-run leftovers irrelevant (bases are random, ≥5 km from
        // Basel, but two bases could still land within a wide radius).
        let near_a = base;
        let near_b = offset(base, 111.0, 0.0); // ~111 m south
        let far = offset(base, 6_000.0, 0.0); // ~6 km north

        let a = create_request(&app, create_payload(new_device_id(), near_a)).await;
        let b = create_request(&app, create_payload(new_device_id(), near_b)).await;
        let _far = create_request(&app, create_payload(new_device_id(), far)).await;

        let uri = format!(
            "/api/v1/help-requests?status=open&near={},{}&radius_m=300",
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
        assert!(
            ids.contains(&a["id"].as_str().unwrap()),
            "near open A in {ids:?}"
        );
        assert!(
            ids.contains(&b["id"].as_str().unwrap()),
            "near open B in {ids:?}"
        );
        assert_eq!(ids.len(), 2, "far request filtered out by radius: {ids:?}");
        for request in listed.as_array().unwrap() {
            assert_anonymous(request);
        }

        // A status that nothing matches proves the status filter applies.
        let uri = format!(
            "/api/v1/help-requests?status=responded&near={},{}&radius_m=300",
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
        let Some(app) = list_app().await else {
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
            let (status, body) = send_json(
                app.clone(),
                "GET",
                &format!("/api/v1/help-requests?{query}"),
                None,
            )
            .await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "expected 422 for ?{query}: {body}"
            );
        }
    }

    /// A helper sharing a live location within the request radius and seen
    /// recently receives `help_request_new` over SSE; helpers outside the
    /// radius, non-helpers and the requester itself receive nothing. Event
    /// payloads carry the anonymous public shape only (ADR 0004).
    #[tokio::test]
    async fn sos_notifies_helpers_with_fresh_live_location_within_radius() {
        let Some(state) = test_state().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let app = router_with_state(&state);
        let addr = spawn_server(app.clone()).await;
        let sos_point = scenario_point();

        let helper_in = new_device_id();
        let helper_out = new_device_id();
        let civilian = new_device_id();
        let requester = new_device_id();
        put_device(&app, helper_in, true, Some(offset(sos_point, 30.0, 0.0))).await;
        put_device(
            &app,
            helper_out,
            true,
            Some(offset(sos_point, 3_000.0, 0.0)),
        )
        .await;
        put_device(&app, civilian, false, Some(offset(sos_point, 20.0, 0.0))).await;

        // Streams open BEFORE the SOS exists so nothing can be missed.
        let mut stream_in = SseStream::connect(addr, helper_in).await;
        let mut stream_out = SseStream::connect(addr, helper_out).await;
        let mut stream_civilian = SseStream::connect(addr, civilian).await;
        let mut stream_requester = SseStream::connect(addr, requester).await;

        let body = create_request(&app, create_payload(requester, sos_point)).await;

        let (event, data) = stream_in.read_event(SSE_EVENT_TIMEOUT).await;
        assert_eq!(event, "help_request_new", "matched helper is notified");
        assert_eq!(data["id"], body["id"]);
        assert_eq!(data["status"], "open");
        assert_anonymous(&data);

        for (stream, who) in [
            (&mut stream_out, "out-of-radius helper"),
            (&mut stream_civilian, "non-helper"),
            (&mut stream_requester, "requester (own SOS)"),
        ] {
            assert!(
                stream.try_read_event(SSE_SILENCE_TIMEOUT).await.is_none(),
                "{who} must not be notified"
            );
        }
    }

    /// A helper with an ACTIVE all-day window on today's Zurich weekday whose
    /// point is within the radius matches without any live location; the
    /// wrong weekday, an inactive window (vacation toggle) and a far-away
    /// window point do not match.
    #[tokio::test]
    async fn sos_notifies_helpers_with_active_all_day_window_today() {
        let Some(state) = test_state().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let app = router_with_state(&state);
        let addr = spawn_server(app.clone()).await;
        let sos_point = scenario_point();
        let today = today_in_zurich();
        let other_day = (today + 1) % 7;

        let helper_today = new_device_id();
        let helper_wrong_day = new_device_id();
        let helper_inactive = new_device_id();
        let helper_far = new_device_id();
        for helper in [helper_today, helper_wrong_day, helper_inactive, helper_far] {
            put_device(&app, helper, true, None).await; // no live location
        }
        create_window(
            &app,
            helper_today,
            today,
            offset(sos_point, 100.0, 0.0),
            true,
        )
        .await;
        create_window(
            &app,
            helper_wrong_day,
            other_day,
            offset(sos_point, 100.0, 0.0),
            true,
        )
        .await;
        create_window(
            &app,
            helper_inactive,
            today,
            offset(sos_point, 100.0, 0.0),
            false,
        )
        .await;
        create_window(
            &app,
            helper_far,
            today,
            offset(sos_point, 3_000.0, 0.0),
            true,
        )
        .await;

        let mut stream_today = SseStream::connect(addr, helper_today).await;
        let mut stream_wrong_day = SseStream::connect(addr, helper_wrong_day).await;
        let mut stream_inactive = SseStream::connect(addr, helper_inactive).await;
        let mut stream_far = SseStream::connect(addr, helper_far).await;

        let body = create_request(&app, create_payload(new_device_id(), sos_point)).await;

        let (event, data) = stream_today.read_event(SSE_EVENT_TIMEOUT).await;
        assert_eq!(event, "help_request_new", "window helper is notified");
        assert_eq!(data["id"], body["id"]);
        assert_anonymous(&data);

        for (stream, who) in [
            (&mut stream_wrong_day, "wrong-weekday window"),
            (&mut stream_inactive, "inactive window"),
            (&mut stream_far, "far window point"),
        ] {
            assert!(
                stream.try_read_event(SSE_SILENCE_TIMEOUT).await.is_none(),
                "{who} must not match"
            );
        }
    }

    /// A helper whose last_seen_at is older than 24 h does not match via its
    /// live location, even when the point is inside the radius. The age is
    /// backdated directly in the database: `last_seen_at` has no API lever
    /// (the devices API always refreshes it) and no clock control exists.
    #[tokio::test]
    async fn sos_ignores_helpers_not_seen_in_last_24_hours() {
        let Some(state) = test_state().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let app = router_with_state(&state);
        let addr = spawn_server(app.clone()).await;
        let sos_point = scenario_point();

        let stale_helper = new_device_id();
        put_device(&app, stale_helper, true, Some(offset(sos_point, 30.0, 0.0))).await;
        sqlx::query("UPDATE devices SET last_seen_at = now() - INTERVAL '25 hours' WHERE id = $1")
            .bind(stale_helper)
            .execute(&state.pool)
            .await
            .expect("backdate works");

        let mut stream = SseStream::connect(addr, stale_helper).await;
        create_request(&app, create_payload(new_device_id(), sos_point)).await;

        assert!(
            stream.try_read_event(SSE_SILENCE_TIMEOUT).await.is_none(),
            "stale helper must not be notified"
        );
    }

    /// The per-request radius override widens matching: a helper at ~800 m is
    /// outside the default 500 m radius but inside a 1 500 m override.
    #[tokio::test]
    async fn sos_radius_override_extends_matching() {
        let Some(state) = test_state().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let app = router_with_state(&state);
        let addr = spawn_server(app.clone()).await;
        let sos_point = scenario_point();

        let helper = new_device_id();
        put_device(&app, helper, true, Some(offset(sos_point, 800.0, 0.0))).await;
        let mut stream = SseStream::connect(addr, helper).await;

        let default_sos = create_request(&app, create_payload(new_device_id(), sos_point)).await;
        assert!(
            stream.try_read_event(SSE_SILENCE_TIMEOUT).await.is_none(),
            "800 m is outside the default 500 m radius"
        );

        let wide = create_request(
            &app,
            json!({
                "device_id": new_device_id(),
                "location": {"lon": sos_point.lon, "lat": sos_point.lat},
                "radius_m": 1500,
            }),
        )
        .await;
        let (event, data) = stream.read_event(SSE_EVENT_TIMEOUT).await;
        assert_eq!(event, "help_request_new");
        assert_eq!(data["id"], wide["id"], "notified about the wide-radius SOS");
        assert_ne!(data["id"], default_sos["id"]);
        assert_anonymous(&data);
    }

    /// The respond transition: the first responder wins, repeating the same
    /// responder's call is idempotent, any other device gets a conflict and
    /// the requester cannot respond to their own SOS. Responder identity
    /// never appears in any body (ADR 0004).
    #[tokio::test]
    async fn respond_first_wins_idempotent_conflicts_for_others() {
        let Some(state) = test_state().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let app = router_with_state(&state);
        let addr = spawn_server(app.clone()).await;
        let sos_point = scenario_point();

        let helper = new_device_id();
        let requester = new_device_id();
        put_device(&app, helper, true, Some(offset(sos_point, 30.0, 0.0))).await;
        let mut stream = SseStream::connect(addr, helper).await;

        let body = create_request(&app, create_payload(requester, sos_point)).await;
        let id = body["id"].as_str().unwrap().to_string();
        let _ = stream.read_event(SSE_EVENT_TIMEOUT).await; // help_request_new

        // First responder wins.
        let (status, responded) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/respond"),
            Some(json!({ "device_id": helper })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "first responder wins: {responded}");
        assert_eq!(responded["status"], "responded");
        assert_anonymous(&responded);

        // The same responder repeating is idempotent.
        let (status, repeat) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/respond"),
            Some(json!({ "device_id": helper })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "same responder repeats: {repeat}");
        assert_eq!(repeat["status"], "responded");

        // A different device gets a conflict.
        let (status, conflict) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/respond"),
            Some(json!({ "device_id": new_device_id() })),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "other device: {conflict}");
        assert!(conflict["error"].is_string());

        // The requester cannot respond to their own SOS.
        let (status, own) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/respond"),
            Some(json!({ "device_id": requester })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "requester self-respond: {own}"
        );

        // Unknown requests are 404.
        let (status, unknown) = send_json(
            app,
            "POST",
            &format!("/api/v1/help-requests/{}/respond", new_device_id()),
            Some(json!({ "device_id": helper })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "unknown request: {unknown}");

        // Exactly one status fan-out reached the responder (the idempotent
        // repeat and the rejected attempts changed nothing).
        let (event, data) = stream.read_event(SSE_EVENT_TIMEOUT).await;
        assert_eq!(event, "help_request_updated");
        assert_eq!(data["id"], body["id"]);
        assert_eq!(data["status"], "responded");
        assert_anonymous(&data);
        assert!(
            stream.try_read_event(SSE_SILENCE_TIMEOUT).await.is_none(),
            "no further events after the single transition"
        );
    }

    /// Status changes fan out as `help_request_updated` to the requester,
    /// the responder and the ORIGINALLY notified helpers - the helper here
    /// moves away after matching and must still hear the fan-out (ADR 0004).
    /// Helpers that never matched hear nothing.
    #[tokio::test]
    async fn status_changes_fan_out_to_requester_responder_and_originally_notified() {
        let Some(state) = test_state().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let app = router_with_state(&state);
        let addr = spawn_server(app.clone()).await;
        let sos_point = scenario_point();

        let helper = new_device_id();
        let outsider = new_device_id();
        let requester = new_device_id();
        put_device(&app, helper, true, Some(offset(sos_point, 30.0, 0.0))).await;
        put_device(&app, outsider, true, Some(offset(sos_point, 3_000.0, 0.0))).await;

        let mut stream_helper = SseStream::connect(addr, helper).await;
        let mut stream_outsider = SseStream::connect(addr, outsider).await;
        let mut stream_requester = SseStream::connect(addr, requester).await;

        let body = create_request(&app, create_payload(requester, sos_point)).await;
        let id = body["id"].as_str().unwrap().to_string();
        let (event, _) = stream_helper.read_event(SSE_EVENT_TIMEOUT).await;
        assert_eq!(event, "help_request_new");
        assert!(stream_requester
            .try_read_event(SSE_SILENCE_TIMEOUT)
            .await
            .is_none());
        assert!(stream_outsider
            .try_read_event(SSE_SILENCE_TIMEOUT)
            .await
            .is_none());

        // The helper moves away AFTER matching - fan-out still reaches it.
        put_device(&app, helper, true, Some(offset(sos_point, 2_000.0, 0.0))).await;

        let (status, responded) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/respond"),
            Some(json!({ "device_id": helper })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "respond after moving: {responded}");

        let (event, data) = stream_requester.read_event(SSE_EVENT_TIMEOUT).await;
        assert_eq!(
            event, "help_request_updated",
            "requester sees the transition"
        );
        assert_eq!(data["status"], "responded");
        assert_anonymous(&data);
        let (event, data) = stream_helper.read_event(SSE_EVENT_TIMEOUT).await;
        assert_eq!(event, "help_request_updated", "responder hears the fan-out");
        assert_eq!(data["status"], "responded");
        assert!(
            stream_outsider
                .try_read_event(SSE_SILENCE_TIMEOUT)
                .await
                .is_none(),
            "unmatched helper must not be fanned out to"
        );

        // Requester resolves; everyone addressed hears it, the moved-away
        // helper included (originally notified set).
        let (status, resolved) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/resolve"),
            Some(json!({ "device_id": requester })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "requester resolves: {resolved}");
        assert_eq!(resolved["status"], "resolved");
        for (stream, who) in [
            (&mut stream_requester, "requester"),
            (&mut stream_helper, "originally notified helper"),
        ] {
            let (event, data) = stream.read_event(SSE_EVENT_TIMEOUT).await;
            assert_eq!(event, "help_request_updated", "{who} hears the resolve");
            assert_eq!(data["status"], "resolved");
            assert_anonymous(&data);
        }
        assert!(stream_outsider
            .try_read_event(SSE_SILENCE_TIMEOUT)
            .await
            .is_none());

        // Re-resolving is idempotent and emits no further events; an
        // uninvolved device cannot resolve.
        let (status, again) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/resolve"),
            Some(json!({ "device_id": requester })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "resolve is idempotent: {again}");
        let (status, forbidden) = send_json(
            app,
            "POST",
            &format!("/api/v1/help-requests/{id}/resolve"),
            Some(json!({ "device_id": outsider })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "uninvolved resolver: {forbidden}"
        );
        for (stream, who) in [
            (&mut stream_requester, "requester"),
            (&mut stream_helper, "helper"),
            (&mut stream_outsider, "outsider"),
        ] {
            assert!(
                stream.try_read_event(SSE_SILENCE_TIMEOUT).await.is_none(),
                "{who} must hear nothing more"
            );
        }
    }

    /// Cancel works only for the requester and only while open; the
    /// responder can resolve a request they responded to.
    #[tokio::test]
    async fn cancel_rules_and_responder_resolve() {
        let Some(state) = test_state().await else {
            skip();
            return;
        };
        let _db = lock_db().await;
        let app = router_with_state(&state);
        let addr = spawn_server(app.clone()).await;
        let sos_point = scenario_point();

        let helper = new_device_id();
        let requester = new_device_id();
        put_device(&app, helper, true, Some(offset(sos_point, 30.0, 0.0))).await;
        let mut stream = SseStream::connect(addr, helper).await;

        // Cancelled while open: only the requester may cancel.
        let body = create_request(&app, create_payload(requester, sos_point)).await;
        let id = body["id"].as_str().unwrap().to_string();
        let _ = stream.read_event(SSE_EVENT_TIMEOUT).await;

        let (status, foreign) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/cancel"),
            Some(json!({ "device_id": helper })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "only requester cancels: {foreign}"
        );

        let (status, cancelled) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/cancel"),
            Some(json!({ "device_id": requester })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "requester cancels: {cancelled}");
        assert_eq!(cancelled["status"], "cancelled");
        assert_anonymous(&cancelled);

        let (status, late) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/respond"),
            Some(json!({ "device_id": helper })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "no respond after cancel: {late}"
        );
        let (status, twice) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/cancel"),
            Some(json!({ "device_id": requester })),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "no second cancel: {twice}");
        let (status, unresolvable) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/resolve"),
            Some(json!({ "device_id": requester })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "no resolve after cancel: {unresolvable}"
        );

        // A responded request can no longer be cancelled but the responder
        // can resolve it.
        let body = create_request(&app, create_payload(requester, sos_point)).await;
        let id = body["id"].as_str().unwrap().to_string();
        let _ = stream.read_event(SSE_EVENT_TIMEOUT).await;
        let (status, responded) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/respond"),
            Some(json!({ "device_id": helper })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "helper responds: {responded}");
        let (status, no_cancel) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/cancel"),
            Some(json!({ "device_id": requester })),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "no cancel after respond: {no_cancel}"
        );
        let (status, resolved) = send_json(
            app.clone(),
            "POST",
            &format!("/api/v1/help-requests/{id}/resolve"),
            Some(json!({ "device_id": helper })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "responder resolves: {resolved}");
        assert_eq!(resolved["status"], "resolved");
        assert_anonymous(&resolved);
    }

    /// The events endpoint rejects a malformed device_id (not a UUID).
    #[tokio::test]
    async fn events_rejects_malformed_device_id() {
        let Some(app) = list_app().await else {
            skip();
            return;
        };
        let status = super::super::test_support::send_status(
            app,
            "GET",
            "/api/v1/events?device_id=not-a-uuid",
        )
        .await;
        assert!(!status.is_success(), "malformed device_id must be rejected");
    }
}
