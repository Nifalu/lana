//! lana app shell.
//!
//! The app is a client of the lana server (ADR 0001): all shared state lives
//! behind the REST API, and this shell owns only an embedded SQLite cache
//! (ADR 0002) plus the Tauri commands the frontend invokes. No Postgres.

mod cache;
mod db;
mod snapshot;
mod sync;

use tauri::{Manager, State};

/// Shared application state managed by Tauri.
pub struct AppState {
    pub pool: sqlx::sqlite::SqlitePool,
    /// The one HTTP client every sync reuses (connection pooling, timeout).
    pub http: reqwest::Client,
}

/// Returns the persistent device id (UUID v4), generating it once and
/// storing it in the cache's settings (ADR 0004: no accounts, no secrets).
#[tauri::command]
async fn get_device_id(state: State<'_, AppState>) -> Result<String, String> {
    cache::get_or_create_device_id(&state.pool)
        .await
        .map_err(|e| e.to_string())
}

/// Returns the configured server base URL (`http://127.0.0.1:8090` unless
/// another server was set).
#[tauri::command]
async fn get_server_url(state: State<'_, AppState>) -> Result<String, String> {
    cache::get_server_url(&state.pool)
        .await
        .map_err(|e| e.to_string())
}

/// Persists the server base URL; rejects non-http(s) values.
#[tauri::command]
async fn set_server_url(state: State<'_, AppState>, url: String) -> Result<(), String> {
    cache::set_server_url(&state.pool, &url)
        .await
        .map_err(|e| e.to_string())
}

/// Pulls the full snapshot from the configured server and replaces the
/// cache wholesale. Returns counts + the snapshot's `generated_at`.
#[tauri::command]
async fn sync_now(state: State<'_, AppState>) -> Result<sync::SyncReport, String> {
    let base_url = cache::get_server_url(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    sync::sync_now(&state.pool, &state.http, &base_url)
        .await
        .map_err(|e| e.to_string())
}

/// Lists cached POIs, optionally filtered by `kind` and/or a `bbox`
/// viewport (`{minLon, minLat, maxLon, maxLat}`).
#[tauri::command]
async fn list_pois(
    state: State<'_, AppState>,
    kind: Option<String>,
    bbox: Option<cache::Bbox>,
) -> Result<Vec<cache::Poi>, String> {
    cache::list_pois(&state.pool, kind.as_deref(), bbox)
        .await
        .map_err(|e| e.to_string())
}

/// Lists cached stations (optionally within a `bbox`) with their latest
/// temperature + measurement timestamp.
#[tauri::command]
async fn list_stations(
    state: State<'_, AppState>,
    bbox: Option<cache::Bbox>,
) -> Result<Vec<cache::Station>, String> {
    cache::list_stations(&state.pool, bbox)
        .await
        .map_err(|e| e.to_string())
}

/// Cache state for the staleness badge: last successful sync and counts.
#[tauri::command]
async fn get_cache_info(state: State<'_, AppState>) -> Result<cache::CacheInfo, String> {
    cache::cache_info(&state.pool)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let pool = tauri::async_runtime::block_on(db::init(&data_dir)).map_err(
                |err| -> Box<dyn std::error::Error> {
                    format!("failed to open the offline cache: {err:#}").into()
                },
            )?;
            let http = sync::http_client().map_err(|err| -> Box<dyn std::error::Error> {
                format!("failed to build the HTTP client: {err:#}").into()
            })?;
            app.manage(AppState { pool, http });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_device_id,
            get_server_url,
            set_server_url,
            sync_now,
            list_pois,
            list_stations,
            get_cache_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running lana");
}
