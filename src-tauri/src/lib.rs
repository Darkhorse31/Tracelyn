mod alerts;
mod apps;
mod collectors;
mod commands;
mod config;
mod connection;
mod diagnostics;
mod events;
mod insight;
mod infra;
mod intel;
mod logs;
mod metrics;
mod ops;
mod metrics_ingest;
mod secrets;
mod setup;
mod ssh;
mod state;
mod storage;
mod webserver;

#[cfg(test)]
mod copy_guard;

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
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .manage(infra::InfraState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let hub = app.state::<AppState>().event_hub.clone();
            hub.set_app(handle.clone());
            hub.clone().spawn_flusher();
            metrics_ingest::spawn_storage_maintenance(handle.clone());
            infra::setup(handle.clone()).map_err(std::io::Error::other)?;
            intel::spawn_scheduler(handle.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            infra::get_infra_snapshot,
            infra::refresh_infra,
            infra::get_infra_target,
            infra::set_infra_target,
            infra::list_infra_domains,
            infra::list_uptime_checks,
            infra::upsert_uptime_check,
            infra::delete_uptime_check,
            infra::run_uptime_check,
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
            commands::subscribe_web_traffic,
            commands::unsubscribe_web_traffic,
            commands::subscribe_apm_metrics,
            commands::unsubscribe_apm_metrics,
            commands::subscribe_redis_metrics,
            commands::unsubscribe_redis_metrics,
            commands::apps::list_apps,
            commands::apps::get_app,
            commands::apps::upsert_manual_app_cmd,
            commands::apps::hide_app,
            commands::apps::merge_apps,
            commands::apps::rediscover_apps,
            commands::apps::subscribe_apps,
            commands::apps::unsubscribe_apps,
            commands::logs::subscribe_log_stream,
            commands::logs::unsubscribe_log_stream,
            commands::logs::get_log_snapshot,
            commands::logs::set_log_paused,
            commands::diagnostics::get_server_diagnostics,
            commands::setup::run_setup_checks,
            commands::setup::list_ssh_private_keys,
            commands::setup::inspect_ssh_private_key,
            commands::insight::list_traces,
            commands::insight::get_trace,
            commands::insight::get_service_map,
            commands::insight::list_error_issues,
            commands::insight::set_error_issue_state,
            commands::insight::list_slow_queries,
            commands::insight::get_insight_setup,
            commands::insight::set_otel_paths,
            commands::insight::set_slowlog_path,
            commands::webtraffic::get_route_rules,
            commands::webtraffic::set_route_rules,
            commands::webtraffic::get_web_traffic_snapshot,
            commands::webtraffic::get_apm_snapshot,
            commands::redis::set_redis_password,
            commands::redis::set_redis_username,
            commands::redis::set_redis_target,
            commands::redis::get_redis_target,
            commands::redis::get_redis_snapshot,
            commands::redis::redis_auth_status,
            commands::redis::test_redis_connection,
            commands::storage_alerts::get_retention_config,
            commands::storage_alerts::set_retention_config,
            commands::storage_alerts::query_metric_series,
            commands::storage_alerts::list_alert_rules,
            commands::storage_alerts::upsert_alert_rule,
            commands::storage_alerts::delete_alert_rule,
            commands::storage_alerts::list_alert_history,
            commands::intel::intel_list_rules,
            commands::intel::intel_upsert_rule,
            commands::intel::intel_delete_rule,
            commands::intel::intel_evaluate,
            commands::intel::intel_list_channels,
            commands::intel::intel_upsert_channel,
            commands::intel::intel_delete_channel,
            commands::intel::intel_test_channel,
            commands::intel::intel_list_incidents,
            commands::intel::intel_get_incident,
            commands::intel::intel_ack_incident,
            commands::intel::intel_resolve_incident,
            commands::intel::intel_add_incident_note,
            commands::intel::intel_anomaly_series,
            commands::intel::intel_list_anomalies,
            commands::intel::intel_export_report,
            commands::intel::intel_export_config,
            commands::intel::intel_import_config,
            commands::intel::intel_list_views,
            commands::intel::intel_upsert_view,
            commands::intel::intel_delete_view,
            commands::intel::intel_list_digests,
            commands::intel::intel_upsert_digest,
            commands::intel::intel_delete_digest,
            commands::intel::intel_send_digest,
            commands::ops::ops_get_mode,
            commands::ops::ops_set_mode,
            commands::ops::ops_set_sudo_password,
            commands::ops::ops_sudo_configured,
            commands::ops::ops_preview,
            commands::ops::ops_run,
            commands::ops::ops_list_audit,
            commands::ops::ops_export_audit,
            commands::ops::ops_terminal_open,
            commands::ops::ops_terminal_write,
            commands::ops::ops_terminal_resize,
            commands::ops::ops_terminal_close,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
