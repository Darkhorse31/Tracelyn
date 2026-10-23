mod collectors;
mod commands;
mod config;
mod connection;
mod events;
mod metrics;
mod secrets;
mod ssh;
mod state;
mod webserver;

use config::ConfigStore;
use connection::ConnectionManager;
use events::EventHub;
use ssh::HostKeyStore;
use state::AppState;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = ConfigStore::load().expect("failed to load config");
    let host_keys = Arc::new(tokio::sync::Mutex::new(
        HostKeyStore::load().expect("failed to load known hosts"),
    ));
    let event_hub = Arc::new(EventHub::new());
    let connections = ConnectionManager::new(host_keys.clone(), event_hub.clone());
    let app_state = AppState::new(config, host_keys, event_hub.clone(), connections);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .setup(|app| {
            let handle = app.handle().clone();
            let hub = app.state::<AppState>().event_hub.clone();
            hub.set_app(handle);
            hub.clone().spawn_flusher();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_servers,
            commands::list_server_statuses,
            commands::upsert_server,
            commands::delete_server,
            commands::test_server_connection,
            commands::connect_server,
            commands::disconnect_server,
            commands::subscribe_overview_metrics,
            commands::unsubscribe_overview_metrics,
            commands::subscribe_server_metrics,
            commands::unsubscribe_server_metrics,
            commands::webtraffic::web_parse_log_line,
            commands::webtraffic::web_detect_server,
            commands::webtraffic::web_parse_config,
            commands::webtraffic::web_normalize_route,
            commands::redis::set_redis_password,
            commands::redis::redis_auth_status,
            commands::redis::redis_snapshot_demo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
