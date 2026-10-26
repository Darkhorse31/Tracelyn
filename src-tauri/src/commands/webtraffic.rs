use crate::state::AppState;
use crate::webserver::{validate_rules, RouteRule};
use tauri::State;

#[tauri::command]
pub fn get_route_rules(state: State<AppState>, server_id: String) -> Vec<RouteRule> {
    state.config.lock().route_rules(&server_id)
}

/// Persists rules; a connected server's pipeline picks them up within one second.
#[tauri::command]
pub fn set_route_rules(
    state: State<AppState>,
    server_id: String,
    rules: Vec<RouteRule>,
) -> Result<Vec<RouteRule>, String> {
    validate_rules(&rules)?;
    let mut config = state.config.lock();
    config
        .set_route_rules(&server_id, rules)
        .map_err(|e| e.to_string())?;
    config.save().map_err(|e| e.to_string())?;
    Ok(config.route_rules(&server_id))
}

#[tauri::command]
pub fn get_web_traffic_snapshot(
    state: State<AppState>,
    server_id: String,
) -> Option<crate::events::payloads::WebTrafficEvent> {
    state.event_hub.last_web_snapshot(&server_id)
}

#[tauri::command]
pub fn get_apm_snapshot(
    state: State<AppState>,
    server_id: String,
) -> Option<crate::events::payloads::ApmMetricsEvent> {
    state.event_hub.last_apm_snapshot(&server_id)
}
