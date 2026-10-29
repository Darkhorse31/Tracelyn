use crate::config::SavedSearch;
use crate::events::LogStreamSnapshot;
use crate::logs::buffer::LogLine;
use crate::logs::trace::line_has_trace;
use crate::ops::shell_quote;
use crate::ssh::run_command;
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

#[tauri::command]
pub fn logs_for_trace(
    state: State<AppState>,
    server_id: String,
    trace_id: String,
    window: u64,
) -> Vec<LogLine> {
    let Some(snap) = state
        .log_window(&server_id)
        .or_else(|| state.event_hub.last_log_snapshot(&server_id))
    else {
        return Vec::new();
    };
    let mut hits: Vec<LogLine> = snap
        .lines
        .into_iter()
        .filter(|line| line_has_trace(&line.raw, &trace_id))
        .collect();
    if window > 0 {
        if let Some(first) = hits.iter().map(|line| line.ts_ms).min() {
            hits.retain(|line| line.ts_ms.abs_diff(first) <= window);
        }
    }
    if hits.len() > 500 {
        hits.truncate(500);
    }
    hits
}

#[tauri::command]
pub async fn search_server_logs(
    state: State<'_, AppState>,
    server_id: String,
    pattern: String,
) -> Result<Vec<String>, String> {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.len() > 200 || pattern.contains('\n') || pattern.contains('\0') {
        return Ok(Vec::new());
    }
    let Some(session) = state.session(&server_id) else {
        return Ok(Vec::new());
    };
    let needle = regex_escape(pattern);
    let quoted = shell_quote(&needle);
    let fixed = shell_quote(pattern);
    let apps = state.apps_for_server(&server_id);
    let mut paths = Vec::new();
    for app in apps {
        for src in app.log_sources {
            let Some(path) = src.path else { continue };
            if path.starts_with('/') && !path.contains('\n') && paths.len() < 8 {
                paths.push(shell_quote(&path));
            }
        }
    }
    let files = if paths.is_empty() {
        String::new()
    } else {
        format!("; grep -F -h -- {} {} | head -n 200", fixed, paths.join(" "))
    };
    let cmd = format!(
        "journalctl -n 200 --no-pager --grep {} 2>/dev/null | head -n 200{files}",
        quoted
    );
    let out = run_command(&session, &cmd).await.map_err(|err| err.to_string())?;
    Ok(out.lines().filter(|line| !line.is_empty()).take(200).map(str::to_string).collect())
}

fn regex_escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if matches!(ch, '\\' | '.' | '^' | '$' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

#[tauri::command]
pub fn list_saved_searches(state: State<AppState>, server_id: String) -> Vec<SavedSearch> {
    state.config.lock().saved_searches(&server_id)
}

#[tauri::command]
pub fn save_saved_search(
    state: State<AppState>,
    server_id: String,
    name: String,
    query: String,
) -> Result<(), String> {
    let name = name.trim();
    let query = query.trim();
    if name.is_empty() || name.len() > 80 || name.contains('\n') {
        return Err("Search name must be 1 to 80 characters.".into());
    }
    if query.is_empty() || query.len() > 500 || query.contains('\n') {
        return Err("Search text must be 1 to 500 characters.".into());
    }
    let mut config = state.config.lock();
    config
        .upsert_saved_search(&server_id, name, query)
        .map_err(|err| err.to_string())?;
    config.save().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn delete_saved_search(
    state: State<AppState>,
    server_id: String,
    name: String,
) -> Result<(), String> {
    let mut config = state.config.lock();
    config
        .delete_saved_search(&server_id, name.trim())
        .map_err(|err| err.to_string())?;
    config.save().map_err(|err| err.to_string())
}
