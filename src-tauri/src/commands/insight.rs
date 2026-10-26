use crate::insight::detect::validate_log_path;
use crate::insight::model::{InsightSetup, ServiceMap, SlowQueryRow, TraceDetail, TraceQuery, TraceSummary};
use crate::insight::store;
use crate::state::AppState;
use tauri::State;

fn store_err(err: impl ToString) -> String {
    err.to_string()
}

#[tauri::command]
pub fn list_traces(state: State<AppState>, query: TraceQuery) -> Result<Vec<TraceSummary>, String> {
    let guard = state.metrics.lock();
    store::list_traces(guard.connection(), &query).map_err(store_err)
}

#[tauri::command]
pub fn get_trace(state: State<AppState>, server_id: String, trace_id: String) -> Result<TraceDetail, String> {
    let guard = state.metrics.lock();
    store::get_trace(guard.connection(), &server_id, &trace_id).map_err(store_err)
}

#[tauri::command]
pub fn get_service_map(
    state: State<AppState>,
    server_id: String,
    start_ms: i64,
    end_ms: i64,
) -> Result<ServiceMap, String> {
    let guard = state.metrics.lock();
    store::service_map(guard.connection(), &server_id, start_ms, end_ms).map_err(store_err)
}

#[tauri::command]
pub fn list_error_issues(
    state: State<AppState>,
    server_id: String,
    app_id: Option<String>,
) -> Result<Vec<crate::insight::model::ErrorIssue>, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let guard = state.metrics.lock();
    store::list_errors(guard.connection(), &server_id, app_id.as_deref(), now).map_err(store_err)
}

#[tauri::command]
pub fn set_error_issue_state(
    state: State<AppState>,
    server_id: String,
    fingerprint: String,
    issue_state: String,
) -> Result<(), String> {
    if !matches!(issue_state.as_str(), "open" | "resolved" | "ignored") {
        return Err("State must be open, resolved, or ignored".into());
    }
    let guard = state.metrics.lock();
    store::set_error_state(guard.connection(), &server_id, &fingerprint, &issue_state).map_err(store_err)
}

#[tauri::command]
pub fn list_slow_queries(
    state: State<AppState>,
    server_id: String,
    engine: Option<String>,
) -> Result<Vec<SlowQueryRow>, String> {
    let guard = state.metrics.lock();
    store::list_slow(guard.connection(), &server_id, engine.as_deref()).map_err(store_err)
}

#[tauri::command]
pub fn get_insight_setup(state: State<AppState>, server_id: String) -> Result<InsightSetup, String> {
    let guard = state.metrics.lock();
    store::setup(guard.connection(), &server_id).map_err(store_err)
}

#[tauri::command]
pub fn set_otel_paths(
    state: State<AppState>,
    server_id: String,
    app_id: String,
    paths: Vec<String>,
) -> Result<(), String> {
    let mut unique = Vec::new();
    for path in paths {
        validate_log_path(&path)?;
        if !unique.contains(&path) {
            unique.push(path);
        }
    }
    let guard = state.metrics.lock();
    store::set_otel_paths(guard.connection(), &server_id, &app_id, &unique).map_err(store_err)
}

#[tauri::command]
pub fn set_slowlog_path(
    state: State<AppState>,
    server_id: String,
    engine: String,
    path: String,
) -> Result<(), String> {
    if !matches!(engine.as_str(), "mysql" | "postgres" | "mongo") {
        return Err("Engine must be mysql, postgres, or mongo".into());
    }
    let guard = state.metrics.lock();
    if path.trim().is_empty() {
        return store::clear_slow_path(guard.connection(), &server_id, &engine).map_err(store_err);
    }
    validate_log_path(&path)?;
    store::set_slow_path(guard.connection(), &server_id, &engine, &path).map_err(store_err)
}
