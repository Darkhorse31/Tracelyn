mod commands;
mod config;
mod secrets;
mod ssh;
mod state;
mod webserver;

use config::ConfigStore;
use ssh::HostKeyStore;
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = ConfigStore::load().expect("failed to load config");
    let host_keys = HostKeyStore::load().expect("failed to load known hosts");
    let app_state = AppState::new(config, host_keys);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::list_servers,
            commands::list_server_statuses,
            commands::upsert_server,
            commands::delete_server,
            commands::test_server_connection,
            commands::connect_server,
            commands::disconnect_server,
            commands::webtraffic::web_parse_log_line,
            commands::webtraffic::web_detect_server,
            commands::webtraffic::web_parse_config,
            commands::webtraffic::web_normalize_route,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
