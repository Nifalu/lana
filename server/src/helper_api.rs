//! Client for the live-location / closest-helpers API (a separate service
//! that owns live device locations and the "who is near this SOS" query).
//!
//! The middleware forwards every shared device location to it
//! ([`HelperApi::post_location`]) and asks it for the helpers closest to a new
//! SOS ([`HelperApi::closest_helpers`]). The service is optional: it is
//! enabled by `LANA_HELPER_API_URL` (see `config`), and callers must treat
//! every failure as non-fatal – the middleware keeps working without it.
//!
//! Wire format (JSON, coordinates `{longitude, latitude}`):
//!
//! - `POST {base}/location?device_id=<uuid>` with a coordinate body → 200;
//! - `POST {base}/get_closest_helpers` with the SOS coordinates → 200 and a
//!   JSON array of `{ "id": <device id>, "distance_m": …, … }`, nearest first.
//!   Only `id` is read; other fields are ignored.

use std::time::Duration;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::api::types::LonLat;

/// How long establishing the TCP/TLS connection may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// How long one whole request may take. Kept short: an SOS waits for the
/// closest-helpers answer before it is stored and fanned out.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// A configured helper API: its base URL plus one shared HTTP client
/// (connection pool included). Cheap to clone.
#[derive(Debug, Clone)]
pub struct HelperApi {
    base_url: String,
    client: reqwest::Client,
}

/// The API's coordinate body.
#[derive(Debug, Serialize)]
struct Coordinates {
    longitude: f64,
    latitude: f64,
}

impl From<LonLat> for Coordinates {
    fn from(point: LonLat) -> Self {
        Self {
            longitude: point.lon,
            latitude: point.lat,
        }
    }
}

/// One `get_closest_helpers` result row; only the device id is used.
#[derive(Debug, Deserialize)]
struct ClosestHelper {
    id: String,
}

impl HelperApi {
    /// Builds a client for the API at `base_url` (a trailing slash is
    /// tolerated).
    pub fn new(base_url: &str) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .context("failed to build the helper API HTTP client")?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        })
    }

    /// The base URL requests are sent to (no trailing slash).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Reports `device_id`'s current location to the API.
    pub async fn post_location(&self, device_id: uuid::Uuid, point: LonLat) -> anyhow::Result<()> {
        self.client
            .post(format!("{}/location", self.base_url))
            .query(&[("device_id", device_id.to_string())])
            .json(&Coordinates::from(point))
            .send()
            .await
            .context("POST /location failed")?
            .error_for_status()
            .context("POST /location was rejected")?;
        Ok(())
    }

    /// The devices the API considers close to an SOS at `point`, nearest
    /// first. Ids that are not UUIDs are skipped. The API knows nothing about
    /// the helper opt-in or the requester, so callers must filter further.
    pub async fn closest_helpers(&self, point: LonLat) -> anyhow::Result<Vec<uuid::Uuid>> {
        let rows: Vec<ClosestHelper> = self
            .client
            .post(format!("{}/get_closest_helpers", self.base_url))
            .json(&Coordinates::from(point))
            .send()
            .await
            .context("POST /get_closest_helpers failed")?
            .error_for_status()
            .context("POST /get_closest_helpers was rejected")?
            .json()
            .await
            .context("POST /get_closest_helpers returned an unexpected body")?;
        Ok(rows.iter().filter_map(|row| row.id.parse().ok()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::test_support::MockHelperApi;
    use serde_json::json;

    fn point() -> LonLat {
        LonLat {
            lon: 7.5886,
            lat: 47.5581,
        }
    }

    /// A trailing slash on the configured URL does not produce `//` paths.
    #[test]
    fn base_url_drops_trailing_slash() {
        let api = HelperApi::new("http://localhost:9000/").unwrap();
        assert_eq!(api.base_url(), "http://localhost:9000");
    }

    /// `post_location` sends `device_id` as a query parameter and the
    /// coordinates as `{longitude, latitude}`.
    #[tokio::test]
    async fn post_location_sends_device_id_query_and_coordinates() {
        let mock = MockHelperApi::spawn().await;
        let api = HelperApi::new(&mock.url()).unwrap();
        let device = uuid::Uuid::new_v4();

        api.post_location(device, point()).await.unwrap();

        let calls = mock.location_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].device_id, device.to_string());
        assert_eq!(
            calls[0].body,
            json!({"longitude": 7.5886, "latitude": 47.5581})
        );
    }

    /// `closest_helpers` returns the UUIDs in API order, skips ids that do
    /// not parse and sends the SOS coordinates.
    #[tokio::test]
    async fn closest_helpers_parses_ids_and_skips_invalid() {
        let mock = MockHelperApi::spawn().await;
        let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        mock.set_closest(vec![a.to_string(), "not-a-uuid".to_string(), b.to_string()]);
        let api = HelperApi::new(&mock.url()).unwrap();

        let ids = api.closest_helpers(point()).await.unwrap();

        assert_eq!(ids, vec![a, b]);
        assert_eq!(
            mock.closest_calls(),
            vec![json!({"longitude": 7.5886, "latitude": 47.5581})]
        );
    }

    /// A non-2xx answer is an error (the caller falls back to local matching).
    #[tokio::test]
    async fn errors_on_non_2xx() {
        let mock = MockHelperApi::spawn().await;
        mock.set_failing(true);
        let api = HelperApi::new(&mock.url()).unwrap();

        assert!(api.closest_helpers(point()).await.is_err());
        assert!(api
            .post_location(uuid::Uuid::new_v4(), point())
            .await
            .is_err());
    }

    /// An unreachable API is an error, not a hang.
    #[tokio::test]
    async fn errors_when_unreachable() {
        let api = HelperApi::new(&MockHelperApi::unreachable_url()).unwrap();
        assert!(api.closest_helpers(point()).await.is_err());
    }
}
