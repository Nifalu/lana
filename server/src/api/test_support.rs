//! Shared helpers for HTTP-seam tests.
//!
//! Tests drive the axum router in-process through `tower::ServiceExt::oneshot`
//! – no sockets, no fixed ports (the port space is shared). Tests that need
//! Postgres are DB-gated: they skip silently when `DATABASE_URL` is unset,
//! following the repo convention (see `db.rs`).

use std::sync::Mutex;
use std::sync::MutexGuard;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

use super::types::LonLat;

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
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = crate::db::init_with_url(&url)
        .await
        .expect("db init failed");
    Some(super::router(pool))
}

/// Standard skip preamble: returns true when the caller should bail out
/// because no database is configured.
pub fn skip_reason() -> bool {
    std::env::var("DATABASE_URL").is_err()
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
            assert!(dlat >= 0.05 && dlon >= 0.05, "too close to Basel: {point:?}");
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
