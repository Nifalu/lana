mod db;

use tauri::{Manager, State};

/// Shared application state managed by Tauri.
pub struct AppState {
    pub pool: sqlx::PgPool,
}

/// Example command – reachable from the frontend via `invoke("greet", { name })`.
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {name}! Greetings from the lana backend.")
}

/// Example command demonstrating DB access from the frontend.
#[tauri::command]
async fn db_version(state: State<'_, AppState>) -> Result<String, String> {
    let (version,): (String,) = sqlx::query_as("SELECT version()")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(version)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let pool = tauri::async_runtime::block_on(db::init())?;
            app.manage(AppState { pool });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, db_version])
        .run(tauri::generate_context!())
        .expect("error while running lana");
}
