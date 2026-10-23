mod collectors;
mod commands;
mod config;
mod secrets;
mod ssh;
mod state;

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
            commands::redis::set_redis_password,
            commands::redis::redis_auth_status,
            commands::redis::redis_snapshot_demo,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
