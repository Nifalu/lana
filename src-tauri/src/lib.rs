mod db;

use std::sync::Mutex;

use tauri::{Manager, State};

/// Shared application state managed by Tauri.
pub struct AppState {
    pub conn: Mutex<rusqlite::Connection>,
}

/// Example command – reachable from the frontend via `invoke("greet", { name })`.
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {name}! Greetings from the lana backend.")
}

/// Example command demonstrating DB access from the frontend.
#[tauri::command]
fn db_user_version(state: State<AppState>) -> Result<u32, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    db::user_version(&conn).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let conn = db::init(&app.path().app_data_dir()?)?;
            app.manage(AppState {
                conn: Mutex::new(conn),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, db_user_version])
        .run(tauri::generate_context!())
        .expect("error while running lana");
}
