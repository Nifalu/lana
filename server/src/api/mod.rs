//! HTTP API: router and endpoints.

pub mod snapshot;

use axum::routing::get;
use axum::Router;
use tower_http::cors::CorsLayer;

/// Builds the application router (CORS is permissive for the prototype).
pub fn router() -> Router {
    Router::new()
        .route("/api/v1/snapshot", get(snapshot::get_snapshot))
        .layer(CorsLayer::permissive())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chrono::DateTime;
    use tower::ServiceExt;

    /// GET /api/v1/snapshot returns 200 with empty GeoJSON FeatureCollections
    /// for pois and stations plus a generated_at timestamp.
    #[tokio::test]
    async fn snapshot_returns_empty_feature_collections_and_generated_at() {
        let app = router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/snapshot")
                    .body(Body::empty())
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
        let app = router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/snapshot")
                    .body(Body::empty())
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
