//! HTTP API: router and endpoints.

pub mod snapshot;

use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use sqlx::PgPool;
use tower_http::cors::CorsLayer;

/// Builds the application router around the shared Postgres pool (CORS is
/// permissive for the prototype).
pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/api/v1/snapshot", get(snapshot::get_snapshot))
        .layer(CorsLayer::permissive())
        .with_state(pool)
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
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

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
                    .body(Body::empty())
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
