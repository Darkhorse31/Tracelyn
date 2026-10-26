use crate::apps::{set_app_hidden, upsert_manual_app, TracelynApp};
use crate::state::AppState;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn list_apps(state: State<AppState>, server_id: String) -> Result<Vec<TracelynApp>, String> {
    Ok(state.apps_for_server(&server_id))
}

#[tauri::command]
pub fn get_app(
    state: State<AppState>,
    server_id: String,
    app_id: String,
) -> Result<Option<TracelynApp>, String> {
    Ok(state
        .apps_for_server(&server_id)
        .into_iter()
        .find(|a| a.id == app_id))
}

#[tauri::command]
pub fn upsert_manual_app_cmd(
    state: State<AppState>,
    app: TracelynApp,
) -> Result<TracelynApp, String> {
    let mut app = app;
    app.origin = crate::apps::types::AppOrigin::Manual;
    let store = state.metrics.lock();
    upsert_manual_app(store.connection(), &app).map_err(|e| e.to_string())?;
    drop(store);
    state.upsert_cached_app(&app);
    let apps = state.apps_for_server(&app.server_id);
    state.event_hub.push_apps(&app.server_id, apps);
    Ok(app)
}

#[tauri::command]
pub fn hide_app(
    state: State<AppState>,
    server_id: String,
    app_id: String,
    hidden: bool,
) -> Result<(), String> {
    let store = state.metrics.lock();
    set_app_hidden(store.connection(), &server_id, &app_id, hidden).map_err(|e| e.to_string())?;
    drop(store);
    state.set_app_hidden_cache(&server_id, &app_id, hidden);
    state
        .event_hub
        .push_apps(&server_id, state.apps_for_server(&server_id));
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeAppsInput {
    pub server_id: String,
    pub primary_id: String,
    pub duplicate_ids: Vec<String>,
}

#[tauri::command]
pub fn merge_apps(state: State<AppState>, input: MergeAppsInput) -> Result<TracelynApp, String> {
    let apps = state.apps_for_server(&input.server_id);
    let primary = apps
        .into_iter()
        .find(|a| a.id == input.primary_id)
        .ok_or_else(|| "primary app not found".to_string())?;
    let mut merged = primary.clone();
    {
        let store = state.metrics.lock();
        let conn = store.connection();
        for dup_id in &input.duplicate_ids {
            if !merged.linked_app_ids.iter().any(|id| id == dup_id) {
                merged.linked_app_ids.push(dup_id.clone());
            }
            set_app_hidden(conn, &input.server_id, dup_id, true).map_err(|e| e.to_string())?;
        }
        upsert_manual_app(conn, &merged).map_err(|e| e.to_string())?;
    }
    for dup_id in &input.duplicate_ids {
        state.set_app_hidden_cache(&input.server_id, dup_id, true);
    }
    state.upsert_cached_app(&merged);
    state
        .event_hub
        .push_apps(&input.server_id, state.apps_for_server(&input.server_id));
    Ok(merged)
}

#[tauri::command]
pub async fn rediscover_apps(
    app: AppHandle,
    state: State<'_, AppState>,
    server_id: String,
) -> Result<Vec<TracelynApp>, String> {
    state.request_app_rediscovery(&server_id);
    let apps = state.apps_for_server(&server_id);
    let _ = app.emit(&format!("apps-updated:{}", server_id), &apps);
    Ok(apps)
}

#[tauri::command]
pub fn subscribe_apps(state: State<AppState>, server_id: String) {
    state.event_hub.subscribe_apps(&server_id);
    state.event_hub.replay_apps(&server_id);
}

#[tauri::command]
pub fn unsubscribe_apps(state: State<AppState>, server_id: String) {
    state.event_hub.unsubscribe_apps(&server_id);
}
