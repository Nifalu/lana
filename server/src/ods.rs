//! Thin HTTP shell for the data.bs.ch Opendatasoft explore API v2.1.
//!
//! This module only *fetches* raw bodies. All parsing lives in
//! [`crate::import`] and [`crate::poller`] as pure functions tested against
//! committed fixtures – nothing here is exercised by tests, and the import
//! and poller never touch the network in tests (see [`DatasetSource`] and
//! [`crate::poller::MeasurementSource`]).
//!
//! API notes (verified 2026-10-02, see the exploration notes):
//! - `/exports/geojson` returns the complete dataset as a GeoJSON
//!   FeatureCollection (no pagination support); used for the static datasets.
//! - `/records` pages with `limit`/`offset` (max limit 100, no `next` links);
//!   stop when a page returns fewer rows than the page size. Always pass an
//!   explicit `order_by` (field id) – the default order is not contractual.

use anyhow::Context;

/// Dataset ids on data.bs.ch.
pub const DATASET_FOUNTAINS: &str = "100008";
pub const DATASET_SWIM_AREAS: &str = "100270";
pub const DATASET_AIR_STATIONS: &str = "100082";
pub const DATASET_AIR_MEASUREMENTS: &str = "100009";
pub const DATASET_RHINE: &str = "100046";
pub const DATASET_POOLS: &str = "100384";

/// Deterministic newest-first sorts for the live datasets – the poller reads
/// only the newest rows, so each dataset's timestamp field (they all name it
/// differently) must be sorted descending.
pub const ORDER_BY_AIR_MEASUREMENTS: &str = "dates_max_date desc";
pub const ORDER_BY_RHINE: &str = "startzeitpunkt desc";
pub const ORDER_BY_POOLS: &str = "zeitpunkt_job desc";

/// Where the Opendatasoft explore API lives unless `LANA_ODS_BASE_URL` says
/// otherwise (overridable for local experiments).
pub const DEFAULT_BASE_URL: &str = "https://data.bs.ch/api/explore/v2.1";

/// Record-page size (the API's maximum) and the deterministic sort for the
/// air-station listing.
const RECORDS_PAGE_SIZE: usize = 100;
const AIR_STATIONS_ORDER_BY: &str = "name_original asc";

pub fn resolve_base_url(env_override: Option<String>) -> String {
    env_override.unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

/// Raw dataset bodies for the import. The production implementation fetches
/// from data.bs.ch; tests feed committed fixture files instead, keeping the
/// import logic offline.
pub(crate) trait DatasetSource {
    async fn fountains(&self) -> anyhow::Result<String>;
    async fn swim_areas(&self) -> anyhow::Result<String>;
    /// All record pages of the air-station dataset, in listing order.
    async fn air_stations(&self) -> anyhow::Result<Vec<String>>;
}

#[derive(Debug, Clone)]
pub struct OdsClient {
    base_url: String,
    http: reqwest::Client,
}

impl OdsClient {
    /// Client configured from `LANA_ODS_BASE_URL` (or the production default).
    pub fn from_env() -> Self {
        Self::new(resolve_base_url(std::env::var("LANA_ODS_BASE_URL").ok()))
    }

    pub fn new(base_url: String) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("lana-import (+https://github.com/; hackathon prototype)")
            .build()
            .expect("reqwest client builds with default settings");
        Self { base_url, http }
    }

    /// GETs the full GeoJSON export of a dataset.
    async fn geojson_export(&self, dataset: &str) -> anyhow::Result<String> {
        let url = format!(
            "{}/catalog/datasets/{}/exports/geojson",
            self.base_url, dataset
        );
        let response = self
            .http
            .get(url)
            .send()
            .await
            .with_context(|| format!("requesting geojson export of dataset {dataset}"))?
            .error_for_status()
            .with_context(|| format!("geojson export of dataset {dataset} failed"))?;
        Ok(response.text().await?)
    }

    /// GETs one page of records.
    async fn records_page(
        &self,
        dataset: &str,
        limit: usize,
        offset: usize,
        order_by: &str,
    ) -> anyhow::Result<String> {
        let url = format!("{}/catalog/datasets/{}/records", self.base_url, dataset);
        let response = self
            .http
            .get(url)
            .query(&[
                ("limit", limit.to_string()),
                ("offset", offset.to_string()),
                ("order_by", order_by.to_string()),
            ])
            .send()
            .await
            .with_context(|| format!("requesting records of dataset {dataset}"))?
            .error_for_status()
            .with_context(|| format!("records of dataset {dataset} failed"))?;
        Ok(response.text().await?)
    }
}

impl DatasetSource for OdsClient {
    async fn fountains(&self) -> anyhow::Result<String> {
        self.geojson_export(DATASET_FOUNTAINS).await
    }

    async fn swim_areas(&self) -> anyhow::Result<String> {
        self.geojson_export(DATASET_SWIM_AREAS).await
    }

    async fn air_stations(&self) -> anyhow::Result<Vec<String>> {
        let mut pages = Vec::new();
        let mut offset = 0;
        loop {
            let page = self
                .records_page(
                    DATASET_AIR_STATIONS,
                    RECORDS_PAGE_SIZE,
                    offset,
                    AIR_STATIONS_ORDER_BY,
                )
                .await?;
            let parsed: serde_json::Value =
                serde_json::from_str(&page).context("records endpoint returned non-JSON")?;
            let rows = parsed["results"].as_array().map_or(0, Vec::len);
            pages.push(page);
            if rows < RECORDS_PAGE_SIZE {
                break;
            }
            offset += rows;
        }
        Ok(pages)
    }
}

/// Live-dataset fetching for the poller: newest-first pages of the three
/// measurement datasets, sized to [`crate::poller::AIR_PAGE_SIZE`] (the
/// API's maximum) so one walk covers the stations with few requests.
impl crate::poller::MeasurementSource for OdsClient {
    async fn air_page(&self, offset: usize) -> anyhow::Result<String> {
        self.records_page(
            DATASET_AIR_MEASUREMENTS,
            crate::poller::AIR_PAGE_SIZE,
            offset,
            ORDER_BY_AIR_MEASUREMENTS,
        )
        .await
    }

    async fn rhine(&self) -> anyhow::Result<String> {
        self.records_page(DATASET_RHINE, 1, 0, ORDER_BY_RHINE).await
    }

    async fn pools(&self) -> anyhow::Result<String> {
        self.records_page(DATASET_POOLS, RECORDS_PAGE_SIZE, 0, ORDER_BY_POOLS)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The base URL defaults to data.bs.ch and an env override wins.
    #[test]
    fn base_url_resolution() {
        assert_eq!(
            resolve_base_url(None),
            "https://data.bs.ch/api/explore/v2.1"
        );
        assert_eq!(
            resolve_base_url(Some("http://localhost:1234/api".to_string())),
            "http://localhost:1234/api"
        );
    }
}
