mod cache;
mod db;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Command wiring lands with the sync/query slices.
    let _ = cache::DEFAULT_SERVER_URL;
}
