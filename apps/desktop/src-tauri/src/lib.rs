//! Network Monitor Tauri backend.

mod history;
mod mock_connections;
mod monitoring;
mod settings;
mod stats;

use std::path::PathBuf;
use std::sync::Arc;

use history::{
    list_domains, list_ip_addresses, list_networks, list_processes, query_connection_history,
};
use mock_connections::{
    seed_mock_and_list_connections, ConnectionsPage, DEFAULT_PAGE_SIZE,
};
use monitoring::{CollectorBackend, MonitoringSession, MonitoringStatusDto};
use settings::{AppSettings, DatabaseInfoDto};
use stats::query_statistics;
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

#[tauri::command]
async fn set_dns_resolution(app: tauri::AppHandle, enabled: bool) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::set_dns_resolution(session.database(), enabled).await
}

#[tauri::command]
async fn set_reverse_dns(app: tauri::AppHandle, enabled: bool) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::set_reverse_dns(session.database(), enabled).await
}

#[tauri::command]
async fn set_ip_enrichment(app: tauri::AppHandle, enabled: bool) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::set_ip_enrichment(session.database(), enabled).await
}

#[tauri::command]
async fn set_asn_info(app: tauri::AppHandle, enabled: bool) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::set_asn_info(session.database(), enabled).await
}

#[tauri::command]
async fn set_country_info(app: tauri::AppHandle, enabled: bool) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::set_country_info(session.database(), enabled).await
}

#[tauri::command]
async fn set_cache_ttl_seconds(
    app: tauri::AppHandle,
    seconds: i64,
) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::set_cache_ttl_seconds(session.database(), seconds).await
}

#[tauri::command]
async fn set_history_retention(
    app: tauri::AppHandle,
    enabled: bool,
    days: i64,
) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::set_history_retention(session.database(), enabled, days).await
}

#[tauri::command]
async fn set_language(app: tauri::AppHandle, language: String) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::set_language(session.database(), language).await
}

#[tauri::command]
async fn set_theme(app: tauri::AppHandle, theme: String) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::set_theme(session.database(), theme).await
}

#[tauri::command]
async fn set_date_time_format(
    app: tauri::AppHandle,
    format: String,
) -> Result<AppSettings, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::set_date_time_format(session.database(), format).await
}

#[tauri::command]
async fn get_database_info(app: tauri::AppHandle) -> Result<DatabaseInfoDto, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    DatabaseInfoDto::load(session.database()).await
}

#[tauri::command]
async fn clear_history(app: tauri::AppHandle) -> Result<DatabaseInfoDto, String> {
    let session = app.state::<Arc<MonitoringSession>>();
    settings::clear_history(session.database()).await
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
            if let Err(err) = tauri::async_runtime::block_on(db.apply_history_retention()) {
                tracing::warn!(error = %err, "history retention on startup failed");
            }
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
            query_connection_history,
            list_processes,
            list_ip_addresses,
            list_domains,
            list_networks,
            query_statistics,
            start_monitoring,
            stop_monitoring,
            pause_monitoring,
            resume_monitoring,
            monitoring_status,
            get_settings,
            set_autostart,
            set_start_monitoring_automatically,
            set_background_mode,
            set_dns_resolution,
            set_reverse_dns,
            set_ip_enrichment,
            set_asn_info,
            set_country_info,
            set_cache_ttl_seconds,
            set_history_retention,
            set_language,
            set_theme,
            set_date_time_format,
            get_database_info,
            clear_history,
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
