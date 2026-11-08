//! Network Monitor Tauri backend.

mod mock_connections;

use std::path::PathBuf;

use mock_connections::{
    seed_mock_and_list_connections, ConnectionsPage, DEFAULT_PAGE_SIZE,
};
use tauri::Manager;

/// Opens the app SQLite database under the platform app-data directory.
async fn open_app_database(app: &tauri::AppHandle) -> Result<database::Database, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("resolve app data dir: {err}"))?;
    std::fs::create_dir_all(&dir).map_err(|err| format!("create app data dir: {err}"))?;
    let path: PathBuf = dir.join("monitor.db");
    database::Database::open(&path)
        .await
        .map_err(|err| format!("open database: {err}"))
}

/// Runs the mock collector into SQLite and returns the first page of connections.
#[tauri::command]
async fn list_mock_connections(
    app: tauri::AppHandle,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<ConnectionsPage, String> {
    let db = open_app_database(&app).await?;
    seed_mock_and_list_connections(
        &db,
        page.unwrap_or(1),
        page_size.unwrap_or(DEFAULT_PAGE_SIZE),
    )
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![list_mock_connections])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
