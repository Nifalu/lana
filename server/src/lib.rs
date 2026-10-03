//! lana backend server library: REST API, open-data import, live-measurement
//! poller.
//!
//! Exposed as a library so other crates can reuse the real pieces (most
//! importantly `api::router` + `db::init_with_url`: the Tauri app's sync
//! tests spawn the actual axum router against a real Postgres). The binary
//! (`main.rs`) is a thin CLI wrapper around this library.

pub mod api;
pub mod cli;
pub mod config;
pub mod db;
pub mod geojson;
pub mod helper_api;
pub mod import;
pub mod ods;
pub mod poller;

#[cfg(test)]
mod test_support;
