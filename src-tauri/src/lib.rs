mod alerts;
mod commands;
mod config;
mod metrics_ingest;
mod secrets;
mod ssh;
mod state;
mod storage;

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
        .plugin(tauri_plugin_notification::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::list_servers,
            commands::list_server_statuses,
            commands::upsert_server,
            commands::delete_server,
            commands::test_server_connection,
            commands::connect_server,
            commands::disconnect_server,
            commands::storage_alerts::get_retention_config,
            commands::storage_alerts::set_retention_config,
            commands::storage_alerts::insert_metric_samples,
            commands::storage_alerts::query_metric_series,
            commands::storage_alerts::run_storage_jobs,
            commands::storage_alerts::list_alert_rules,
            commands::storage_alerts::upsert_alert_rule,
            commands::storage_alerts::delete_alert_rule,
            commands::storage_alerts::list_alert_history,
            commands::storage_alerts::evaluate_alerts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
