//! Network Monitor Tauri backend.

mod mock_connections;
mod monitoring;
mod settings;

use std::path::PathBuf;
use std::sync::Arc;

use mock_connections::{
    seed_mock_and_list_connections, ConnectionsPage, DEFAULT_PAGE_SIZE,
};
use monitoring::{CollectorBackend, MonitoringSession, MonitoringStatusDto};
use settings::AppSettings;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt as AutostartExt;

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
    let session = app.state::<Arc<MonitoringSession>>();
    seed_mock_and_list_connections(
        session.database(),
        page.unwrap_or(1),
        page_size.unwrap_or(DEFAULT_PAGE_SIZE),
    )
    .await
}

#[tauri::command]
async fn start_monitoring(
    app: tauri::AppHandle,
    use_mock: Option<bool>,
) -> Result<MonitoringStatusDto, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    let backend = if use_mock.unwrap_or(false) {
        CollectorBackend::Mock
    } else {
        CollectorBackend::Native
    };
    session.start(backend).await
}

#[tauri::command]
async fn stop_monitoring(app: tauri::AppHandle) -> Result<MonitoringStatusDto, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    session.stop().await
}

#[tauri::command]
async fn pause_monitoring(app: tauri::AppHandle) -> Result<MonitoringStatusDto, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    session.pause().await
}

#[tauri::command]
async fn resume_monitoring(app: tauri::AppHandle) -> Result<MonitoringStatusDto, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    session.resume().await
}

#[tauri::command]
async fn monitoring_status(app: tauri::AppHandle) -> Result<MonitoringStatusDto, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    Ok(session.status().await)
}

#[tauri::command]
async fn get_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    AppSettings::load(session.database()).await
}

#[tauri::command]
async fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    let settings = settings::set_autostart_setting(session.database(), enabled).await?;
    let autolaunch = app.autolaunch();
    if enabled {
        autolaunch
            .enable()
            .map_err(|err| format!("enable autostart: {err}"))?;
    } else {
        autolaunch
            .disable()
            .map_err(|err| format!("disable autostart: {err}"))?;
    }
    Ok(settings)
}

#[tauri::command]
async fn set_start_monitoring_automatically(
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::set_start_monitoring_automatically(session.database(), enabled).await
}

#[tauri::command]
async fn set_background_mode(app: tauri::AppHandle, enabled: bool) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::set_background_mode(session.database(), enabled).await
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let handle = app.handle().clone();
            let db = tauri::async_runtime::block_on(open_app_database(&handle))
                .map_err(|err| std::io::Error::other(err))?;
            let app_settings = tauri::async_runtime::block_on(AppSettings::load(&db))
                .map_err(|err| std::io::Error::other(err))?;
            let session = Arc::new(MonitoringSession::new(db));

            // Sync OS autostart with the persisted setting.
            let autolaunch = handle.autolaunch();
            let _ = if app_settings.autostart {
                autolaunch.enable()
            } else {
                autolaunch.disable()
            };

            if app_settings.start_monitoring_automatically {
                let session_clone = Arc::clone(&session);
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = session_clone.start(CollectorBackend::Native).await {
                        tracing::warn!(error = %err, "auto-start monitoring failed");
                    }
                });
            }

            app.manage(session);

            let show_item = MenuItem::with_id(app, "show", "Show Network Monitor", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().ok_or_else(|| {
                    std::io::Error::other("missing default window icon for tray")
                })?)
                .menu(&menu)
                .tooltip("Network Monitor")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "quit" => {
                        // Stop the collector before exiting.
                        let session = Arc::clone(&*app.state::<Arc<MonitoringSession>>());
                        tauri::async_runtime::block_on(async move {
                            let _ = session.stop().await;
                        });
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Limitation: tray uses the app icon (not a dedicated monochrome template).
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle().clone();
                let should_hide = tauri::async_runtime::block_on(async {
                    let session = app.state::<Arc<MonitoringSession>>();
                    AppSettings::load(session.database())
                        .await
                        .map(|s| s.background_mode)
                        .unwrap_or(true)
                });
                if should_hide {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    // Quitting via window close: stop collector first.
                    let session = Arc::clone(&*app.state::<Arc<MonitoringSession>>());
                    tauri::async_runtime::block_on(async move {
                        let _ = session.stop().await;
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_mock_connections,
            start_monitoring,
            stop_monitoring,
            pause_monitoring,
            resume_monitoring,
            monitoring_status,
            get_settings,
            set_autostart,
            set_start_monitoring_automatically,
            set_background_mode,
        ]);

    builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let RunEvent::Exit = event {
                let session = Arc::clone(&*app_handle.state::<Arc<MonitoringSession>>());
                tauri::async_runtime::block_on(async move {
                    let _ = session.stop().await;
                });
            }
        });
}
