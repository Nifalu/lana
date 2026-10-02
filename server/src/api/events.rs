//! SSE event stream (ADR 0003): one endpoint, filtered per device.
//!
//! Slice 2 implements the hub and stream; this skeleton only fixes the seam.

use axum::http::StatusCode;
use axum::extract::State;

use super::AppState;

/// In-memory fan-out hub: routes notifications to the SSE connections of the
/// addressed devices.
#[derive(Clone, Default)]
pub struct Hub;

impl Hub {
    pub fn new() -> Self {
        Self
    }
}

/// `GET /api/v1/events?device_id=<uuid>` – the device's notification stream.
pub async fn events(State(_state): State<AppState>) -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
