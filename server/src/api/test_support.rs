//! Shared helpers for HTTP-seam tests.
//!
//! Tests drive the axum router in-process through `tower::ServiceExt::oneshot`
//! – no sockets, no fixed ports (the port space is shared). Tests that need
//! Postgres are DB-gated: they skip silently when `DATABASE_URL` is unset,
//! following the repo convention (see `db.rs`).

use std::net::SocketAddr;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::tcp::OwnedReadHalf;
use tower::ServiceExt;

use super::types::LonLat;
use super::AppState;
use crate::helper_api::HelperApi;

static DB_LOCK: Mutex<()> = Mutex::new(());

/// Serializes DB-gated API tests that need an empty-ish, stable database view
/// (they share one per-ticket database without truncating each other's rows).
pub async fn lock_db() -> MutexGuard<'static, ()> {
    match DB_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Builds the fully migrated app against the database at `DATABASE_URL`.
/// Returns `None` (the test skips) when `DATABASE_URL` is unset.
pub async fn test_app() -> Option<Router> {
    test_state().await.map(|state| router_with_state(&state))
}

/// Builds the fully migrated app state (pool + fresh hub) against the
/// database at `DATABASE_URL`. Returns `None` (the test skips) when
/// `DATABASE_URL` is unset. Cloning the state shares one hub across every
/// router built from it, so SSE assertions observe oneshot-driven publishes.
pub async fn test_state() -> Option<AppState> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = crate::db::init_with_url(&url)
        .await
        .expect("db init failed");
    Some(AppState::new(pool))
}

/// Like [`test_state`], with the helper API pointed at `helper_api_url`.
pub async fn test_state_with_helper_api(helper_api_url: &str) -> Option<AppState> {
    let helper_api = HelperApi::new(helper_api_url).expect("helper API client builds");
    test_state()
        .await
        .map(|state| state.with_helper_api(Some(helper_api)))
}

/// Builds a router sharing `state`'s pool and hub.
pub fn router_with_state(state: &AppState) -> Router {
    super::router_with_state(state.clone())
}

/// Serves `app` on an ephemeral port (127.0.0.1:0 – never a fixed port, the
/// port space is shared) for tests that need a real streaming connection
/// (SSE). Returns the bound address.
pub async fn spawn_server(app: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("ephemeral bind works");
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("server task failed");
    });
    addr
}

/// An open SSE connection to `GET /api/v1/events?device_id=…`: the request
/// is sent and the response headers consumed (asserted 200 event-stream).
/// Opening is bounded by [`SSE_CONNECT_TIMEOUT`] so a broken endpoint fails
/// the test instead of hanging it (and, via `lock_db`, the whole suite).
pub struct SseStream {
    reader: BufReader<OwnedReadHalf>,
    /// Kept alive for the connection's lifetime: dropping tokio's write half
    /// shuts down the write side (TCP FIN), which would end the SSE stream.
    _write: tokio::net::tcp::OwnedWriteHalf,
}

/// How long opening one SSE stream may take in tests.
const SSE_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

impl SseStream {
    pub async fn connect(addr: SocketAddr, device_id: uuid::Uuid) -> Self {
        use tokio::io::AsyncWriteExt;
        let open = async {
            let stream = tokio::net::TcpStream::connect(addr)
                .await
                .expect("SSE connect works");
            let (read, mut write) = stream.into_split();
            let request = format!(
                "GET /api/v1/events?device_id={device_id} HTTP/1.1\r\nHost: {addr}\r\nAccept: text/event-stream\r\n\r\n"
            );
            write
                .write_all(request.as_bytes())
                .await
                .expect("SSE request write works");
            let mut reader = BufReader::new(read);
            let mut status_line = String::new();
            reader
                .read_line(&mut status_line)
                .await
                .expect("SSE response arrives");
            assert!(
                status_line.contains(" 200 "),
                "SSE endpoint must answer 200, got {status_line:?}"
            );
            let mut content_type = String::new();
            loop {
                let mut line = String::new();
                reader
                    .read_line(&mut line)
                    .await
                    .expect("SSE headers arrive");
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if line.to_ascii_lowercase().starts_with("content-type:") {
                    content_type = line;
                }
            }
            assert!(
                content_type.contains("text/event-stream"),
                "SSE content-type expected, got {content_type:?}"
            );
            Self {
                reader,
                _write: write,
            }
        };
        tokio::time::timeout(SSE_CONNECT_TIMEOUT, open)
            .await
            .expect("SSE connect completes in time")
    }

    /// Reads the next `event:`/`data:` pair, skipping keep-alive comments.
    /// Panics when the event does not arrive within `timeout` – use
    /// [`try_read_event`] for negative assertions.
    pub async fn read_event(&mut self, timeout: Duration) -> (String, Value) {
        self.try_read_event(timeout)
            .await
            .unwrap_or_else(|| panic!("no SSE event within {timeout:?}"))
    }

    /// Like [`read_event`], but returns `None` on timeout instead of
    /// panicking: the assertion for "this device is NOT notified".
    pub async fn try_read_event(&mut self, timeout: Duration) -> Option<(String, Value)> {
        let read = async {
            let mut name = String::from("message");
            let mut data = String::new();
            loop {
                let mut line = String::new();
                let n = self
                    .reader
                    .read_line(&mut line)
                    .await
                    .expect("SSE stream stays readable");
                assert!(n > 0, "SSE stream ended unexpectedly");
                let line = line.trim_end_matches(['\r', '\n']);
                if line.is_empty() {
                    if data.is_empty() {
                        continue; // bare keep-alive separator
                    }
                    let parsed: Value = serde_json::from_str(&data).expect("SSE data is JSON");
                    return (name, parsed);
                }
                if let Some(rest) = line.strip_prefix("event: ") {
                    name = rest.to_string();
                } else if let Some(rest) = line.strip_prefix("data: ") {
                    data = rest.to_string();
                }
                // Other lines (`:` comments) are ignored.
            }
        };
        tokio::time::timeout(timeout, read).await.ok()
    }
}

/// One `POST /location` call as the mock helper API recorded it.
#[derive(Debug, Clone)]
pub struct LocationCall {
    /// The `device_id` query parameter.
    pub device_id: String,
    /// The JSON body (`{longitude, latitude}`).
    pub body: Value,
}

#[derive(Default)]
struct MockState {
    locations: Vec<LocationCall>,
    closest_calls: Vec<Value>,
    closest_ids: Vec<String>,
    failing: bool,
}

/// A stand-in for the live-location / closest-helpers API: a tiny axum
/// server on an ephemeral port that records `POST /location` calls and
/// answers `POST /get_closest_helpers` with a configurable id list – or with
/// 500 on both endpoints in failing mode.
pub struct MockHelperApi {
    addr: SocketAddr,
    state: std::sync::Arc<Mutex<MockState>>,
}

impl MockHelperApi {
    pub async fn spawn() -> Self {
        use axum::extract::{Query, State};
        use axum::routing::post;
        use axum::Json;
        use std::collections::HashMap;
        use std::sync::Arc;

        type Shared = Arc<Mutex<MockState>>;

        async fn location(
            State(state): State<Shared>,
            Query(query): Query<HashMap<String, String>>,
            Json(body): Json<Value>,
        ) -> StatusCode {
            let mut state = state.lock().unwrap();
            state.locations.push(LocationCall {
                device_id: query.get("device_id").cloned().unwrap_or_default(),
                body,
            });
            if state.failing {
                StatusCode::INTERNAL_SERVER_ERROR
            } else {
                StatusCode::OK
            }
        }

        async fn closest(
            State(state): State<Shared>,
            Json(body): Json<Value>,
        ) -> Result<Json<Value>, StatusCode> {
            let mut state = state.lock().unwrap();
            state.closest_calls.push(body);
            if state.failing {
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
            let rows: Vec<Value> = state
                .closest_ids
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    serde_json::json!({
                        "id": id,
                        "distance_m": 10.0 * (i + 1) as f64,
                        "location_updated_at": "2026-10-03T12:00:00Z",
                        "location_age": "PT1M",
                    })
                })
                .collect();
            Ok(Json(Value::Array(rows)))
        }

        let state: Shared = Arc::default();
        let app = Router::new()
            .route("/location", post(location))
            .route("/get_closest_helpers", post(closest))
            .with_state(state.clone());
        let addr = spawn_server(app).await;
        Self { addr, state }
    }

    /// Base URL of the mock (no trailing slash).
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// A base URL nothing listens on: the port was just free, then released.
    pub fn unreachable_url() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("ephemeral bind works");
        let addr = listener.local_addr().unwrap();
        drop(listener);
        format!("http://{addr}")
    }

    /// The ids `POST /get_closest_helpers` answers with, nearest first.
    pub fn set_closest(&self, ids: Vec<String>) {
        self.state.lock().unwrap().closest_ids = ids;
    }

    /// Switches the 500 mode on or off for both endpoints.
    pub fn set_failing(&self, failing: bool) {
        self.state.lock().unwrap().failing = failing;
    }

    pub fn location_calls(&self) -> Vec<LocationCall> {
        self.state.lock().unwrap().locations.clone()
    }

    /// The request bodies `POST /get_closest_helpers` received.
    pub fn closest_calls(&self) -> Vec<Value> {
        self.state.lock().unwrap().closest_calls.clone()
    }

    /// Waits (polling) until at least `count` location calls arrived – the
    /// middleware forwards from a spawned task – and returns all of them.
    pub async fn wait_for_location_calls(&self, count: usize) -> Vec<LocationCall> {
        for _ in 0..100 {
            let calls = self.location_calls();
            if calls.len() >= count {
                return calls;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        self.location_calls()
    }
}

/// Standard skip preamble for DB-gated tests: use as
/// `let Some(app) = test_app().await else { skip(); return; };`.
pub fn skip() {
    eprintln!("DATABASE_URL not set \u{2013} skipping postgres test");
}

/// Sends a request with an optional JSON body and returns the status plus
/// the parsed body (a missing/empty body parses to `Value::Null`).
pub async fn send_json(
    app: Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    let request = match body {
        Some(json) => builder.body(Body::from(json.to_string())),
        None => builder.body(Body::empty()),
    }
    .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response body is JSON")
    };
    (status, json)
}

/// Sends a request with a raw string body (content-type application/json)
/// and returns the status plus the raw response bytes – for rejections whose
/// body shape is itself under test (e.g. malformed JSON must still produce
/// the uniform JSON error, not plain text).
pub async fn send_raw(
    app: Router,
    method: &str,
    uri: &str,
    body: &str,
) -> (StatusCode, axum::body::Bytes) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, bytes)
}

/// Sends a request and returns only its status, ignoring the body (use when
/// the rejection body is not JSON, e.g. axum's plain-text query rejections).
pub async fn send_status(app: Router, method: &str, uri: &str) -> StatusCode {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    response.status()
}

/// A fresh random device UUID, unique per call so parallel tests never
/// collide on rows.
pub fn new_device_id() -> uuid::Uuid {
    uuid::Uuid::new_v4()
}

/// A scenario point unique per call and far away from Basel's default test
/// coordinates (7.5886, 47.5596): derived from a fresh UUID so concurrent
/// tests never share a spot, at least ~0.05° (~5.5 km) away from it so no
/// other test's helper can fall into a scenario radius.
pub fn scenario_point() -> LonLat {
    let bytes = *uuid::Uuid::new_v4().as_bytes();
    let fraction = |chunk: [u8; 4]| -> f64 {
        0.05 + (u32::from_be_bytes(chunk) % 250_000) as f64 / 1_000_000.0
    };
    LonLat {
        lon: 7.5886 + fraction([bytes[0], bytes[1], bytes[2], bytes[3]]),
        lat: 47.5596 + fraction([bytes[4], bytes[5], bytes[6], bytes[7]]),
    }
}

/// A point `north_m` meters north and `east_m` meters east of `base`
/// (good to a few percent at Basel's latitude – enough for radius tests that
/// stay far away from any boundary).
pub fn offset(base: LonLat, north_m: f64, east_m: f64) -> LonLat {
    let meters_per_degree_lat = 111_320.0;
    let meters_per_degree_lon = 111_320.0 * base.lat.to_radians().cos();
    LonLat {
        lon: base.lon + east_m / meters_per_degree_lon,
        lat: base.lat + north_m / meters_per_degree_lat,
    }
}

/// Fails the test when any JSON key anywhere in `value` hints at an identity:
/// the SOS API never exposes requester or responder identity beyond what the
/// status transitions already show (ADR 0004 anonymity).
pub fn assert_anonymous(value: &Value) {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                let key = key.to_lowercase();
                for forbidden in ["requester", "responder", "device"] {
                    assert!(
                        !key.contains(forbidden),
                        "identity leak: key '{key}' in {value}"
                    );
                }
                assert_anonymous(nested);
            }
        }
        Value::Array(items) => items.iter().for_each(assert_anonymous),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scenario_points_are_unique_and_far_from_basel() {
        let a = scenario_point();
        let b = scenario_point();
        assert_ne!(a, b);
        for point in [a, b] {
            let dlat = (point.lat - 47.5596).abs();
            let dlon = (point.lon - 7.5886).abs();
            assert!(
                dlat >= 0.05 && dlon >= 0.05,
                "too close to Basel: {point:?}"
            );
            assert!(point.lat < 90.0 && point.lon < 180.0);
        }
    }

    #[test]
    fn anonymity_check_catches_identity_keys_anywhere() {
        assert_anonymous(&json!({"id": "x", "status": "open"}));
        assert_anonymous(&json!([{"note": null}, {"location": {"lon": 1.0}}]));
        for leak in [
            json!({"requester_id": "x"}),
            json!({"responder": "x"}),
            json!({"nested": [{"device_id": "x"}]}),
            json!({"DEVICE": "x"}),
        ] {
            let result = std::panic::catch_unwind(|| assert_anonymous(&leak));
            assert!(result.is_err(), "should have caught {leak}");
        }
    }
}
