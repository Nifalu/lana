//! HTTP API: router and endpoints.

pub mod devices;
pub mod error;
pub mod events;
pub mod help_requests;
pub mod snapshot;
#[cfg(test)]
pub(crate) mod test_support;
pub mod types;
pub mod windows;

use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use axum::Router;
use sqlx::PgPool;
use tower_http::cors::CorsLayer;

/// Shared handler state: the Postgres pool plus the in-memory SSE hub that
/// routes notifications to connected devices (ADR 0003).
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub hub: events::Hub,
}

/// Builds the application router around the shared Postgres pool (CORS is
/// permissive for the prototype) with a fresh notification hub.
///
/// Identity note: the `device_id` in a request path *is* the caller (ADR 0004
/// – no accounts, no secrets). Every devices/windows handler scopes its SQL to
/// that id, so a device can only ever read or change its own rows.
pub fn router(pool: PgPool) -> Router {
    router_with_state(AppState {
        pool,
        hub: events::Hub::new(),
    })
}

/// Builds the router around an explicit state (used by tests to share one
/// hub across assertions).
pub fn router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/snapshot", get(snapshot::get_snapshot))
        .route("/api/v1/devices/{device_id}", put(devices::upsert_device))
        .route(
            "/api/v1/devices/{device_id}/windows",
            get(windows::list_windows).post(windows::create_window),
        )
        .route(
            "/api/v1/devices/{device_id}/windows/{window_id}",
            patch(windows::patch_window).delete(windows::delete_window),
        )
        .route(
            "/api/v1/help-requests",
            get(help_requests::list_help_requests).post(help_requests::create_help_request),
        )
        .route(
            "/api/v1/help-requests/{request_id}/respond",
            post(help_requests::respond_help_request),
        )
        .route(
            "/api/v1/help-requests/{request_id}/resolve",
            post(help_requests::resolve_help_request),
        )
        .route(
            "/api/v1/help-requests/{request_id}/cancel",
            post(help_requests::cancel_help_request),
        )
        .route("/api/v1/events", get(events::events))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Handler-level error: anything goes wrong → 500 with the error chain as
/// body (adequate for the prototype; the data is public).
pub struct ApiError(anyhow::Error);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("{:#}", self.0),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        Self(err.into())
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        Self(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Request, StatusCode};
    use chrono::DateTime;
    use tower::ServiceExt;

    /// GET /api/v1/snapshot returns 200 with empty GeoJSON FeatureCollections
    /// for pois and stations plus a generated_at timestamp. DB-gated: the
    /// router now carries the Postgres pool (ticket 04 devices/windows).
    #[tokio::test]
    async fn snapshot_returns_empty_feature_collections_and_generated_at() {
        let Some(app) = test_support::test_app().await else {
            eprintln!("DATABASE_URL not set – skipping postgres test");
            return;
        };

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/snapshot")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Structural assertions only: whether pois/stations are empty depends
        // on whether the import (ticket 02) has run against this shared,
        // persistent per-ticket database – an emptiness claim can never hold
        // there once it has.
        assert_eq!(json["pois"]["type"], "FeatureCollection");
        assert!(json["pois"]["features"].is_array());
        assert_eq!(json["stations"]["type"], "FeatureCollection");
        assert!(json["stations"]["features"].is_array());

        let generated_at = json["generated_at"]
            .as_str()
            .expect("generated_at is a string");
        DateTime::parse_from_rfc3339(generated_at).expect("generated_at parses as RFC 3339");
    }

    /// The permissive CORS policy is visible on responses even when the
    /// database is unreachable (handler errors must not strip the headers).
    /// The pool is created lazily against a closed port, so no server is
    /// needed and no fixed port is bound.
    #[tokio::test]
    async fn snapshot_response_carries_permissive_cors_header() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://lana:lana@127.0.0.1:1/does_not_exist")
            .unwrap();
        let app = router(pool);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/snapshot")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );
    }
}
