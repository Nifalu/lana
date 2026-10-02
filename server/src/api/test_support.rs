//! Shared helpers for HTTP-seam tests.
//!
//! Tests drive the axum router in-process through `tower::ServiceExt::oneshot`
//! – no sockets, no fixed ports (the port space is shared). Tests that need
//! Postgres are DB-gated: they skip silently when `DATABASE_URL` is unset,
//! following the repo convention (see `db.rs`).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

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
