//! API error responses: one uniform JSON shape (`{"error": "…"}`).

use axum::extract::FromRequest;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::de::DeserializeOwned;

/// JSON extractor that maps deserialization failures onto the uniform error
/// shape (422 `{"error": …}`) instead of axum's plain-text rejections – a
/// malformed payload is a validation failure, not an axum-internal one.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(Self::Rejection::Validation(rejection.body_text())),
        }
    }
}

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
