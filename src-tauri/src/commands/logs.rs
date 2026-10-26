use crate::events::LogStreamSnapshot;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn subscribe_log_stream(
    state: State<AppState>,
    server_id: String,
    app_ids: Option<Vec<String>>,
    sources: Option<Vec<String>>,
) {
    state.set_log_filter(
        &server_id,
        app_ids.unwrap_or_default(),
        sources.unwrap_or_default(),
    );
    state.event_hub.subscribe_log_stream(&server_id);
}

#[tauri::command]
pub fn set_log_paused(state: State<AppState>, server_id: String, paused: bool) {
    state.set_log_paused(&server_id, paused);
}

#[tauri::command]
pub fn unsubscribe_log_stream(state: State<AppState>, server_id: String) {
    state.event_hub.unsubscribe_log_stream(&server_id);
}

#[tauri::command]
pub fn get_log_snapshot(state: State<AppState>, server_id: String) -> Option<LogStreamSnapshot> {
    state
        .log_window(&server_id)
        .or_else(|| state.event_hub.last_log_snapshot(&server_id))
}
