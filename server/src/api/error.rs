//! API error responses: one uniform JSON shape (`{"error": "…"}`).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

/// Errors that map onto HTTP status codes.
///
/// Unknown and foreign resources are reported identically as 404: a device
/// must not be able to probe whether another device's row exists (ADR 0004).
#[derive(Debug)]
pub enum ApiError {
    NotFound(&'static str),
    Validation(String),
    Database(sqlx::Error),
}

impl ApiError {
    /// Turns a validation failure (`Err` message) into a 422 response.
    pub fn check(result: Result<(), String>) -> Result<(), Self> {
        result.map_err(Self::Validation)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::NotFound(what) => (StatusCode::NOT_FOUND, what.to_string()),
            Self::Validation(message) => (StatusCode::UNPROCESSABLE_ENTITY, message),
            Self::Database(err) => {
                eprintln!("database error: {err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error".to_string(),
                )
            }
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
