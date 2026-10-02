//! HTTP API: router and endpoints.

pub mod devices;
pub mod error;
pub mod snapshot;
#[cfg(test)]
pub(crate) mod test_support;
pub mod types;
pub mod windows;

use axum::routing::{get, patch, post, put};
use axum::Router;
use sqlx::PgPool;
use tower_http::cors::CorsLayer;

/// Builds the application router (CORS is permissive for the prototype).
///
/// Identity note: the `device_id` in a request path *is* the caller (ADR 0004
/// – no accounts, no secrets). Every devices/windows handler scopes its SQL to
/// that id, so a device can only ever read or change its own rows.
pub fn router(pool: PgPool) -> Router {
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
        .with_state(pool)
        .layer(CorsLayer::permissive())
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

        assert_eq!(json["pois"]["type"], "FeatureCollection");
        assert_eq!(json["pois"]["features"], serde_json::json!([]));
        assert_eq!(json["stations"]["type"], "FeatureCollection");
        assert_eq!(json["stations"]["features"], serde_json::json!([]));

        let generated_at = json["generated_at"]
            .as_str()
            .expect("generated_at is a string");
        DateTime::parse_from_rfc3339(generated_at).expect("generated_at parses as RFC 3339");
    }

    /// The permissive CORS policy is visible on normal responses.
    #[tokio::test]
    async fn snapshot_response_carries_permissive_cors_header() {
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

        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("*")
        );
    }
}
