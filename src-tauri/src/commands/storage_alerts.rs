use crate::alerts::{AlertEventRecord, AlertHistoryStore, AlertRule, delete_rule, list_rules, upsert_rule};
use crate::state::AppState;
use crate::storage::{query_metrics, QueryResult, RetentionConfig};
use serde::Deserialize;
use tauri::State;

fn map_err(err: impl ToString) -> String {
    err.to_string()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryMetricsInput {
    pub server_id: String,
    pub metric_key: String,
    pub range_start_ms: i64,
    pub range_end_ms: i64,
    pub prefer_live: bool,
}

#[tauri::command]
pub fn get_retention_config(state: State<AppState>) -> RetentionConfig {
    state.metrics.lock().retention().clone()
}

#[tauri::command]
pub fn set_retention_config(
    state: State<AppState>,
    config: RetentionConfig,
) -> Result<RetentionConfig, String> {
    config.validate()?;
    let mut store = state.metrics.lock();
    store.set_retention(config).map_err(map_err)?;
    Ok(store.retention().clone())
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsAround {
    pub cpu: QueryResult,
    pub memory: QueryResult,
    pub load: QueryResult,
}

#[tauri::command]
pub async fn metrics_around(
    state: State<'_, AppState>,
    server_id: String,
    timestamp_ms: i64,
    window_ms: i64,
) -> Result<MetricsAround, String> {
    let window = window_ms.clamp(1_000, 3_600_000);
    let start = timestamp_ms.saturating_sub(window);
    let end = timestamp_ms.saturating_add(window);
    let load = |metric_key: &str| {
        let store = state.metrics.lock();
        query_metrics(&store, &server_id, metric_key, start, end, true).map_err(map_err)
    };
    Ok(MetricsAround {
        cpu: crate::companion_history::merge(&state, load("cpu.percent")?, start, end).await,
        memory: crate::companion_history::merge(&state, load("memory.percent")?, start, end).await,
        load: crate::companion_history::merge(&state, load("load.1")?, start, end).await,
    })
}

#[tauri::command]
pub async fn query_metric_series(
    state: State<'_, AppState>,
    input: QueryMetricsInput,
) -> Result<QueryResult, String> {
    let local = query_metrics(
        &*state.metrics.lock(),
        &input.server_id,
        &input.metric_key,
        input.range_start_ms,
        input.range_end_ms,
        input.prefer_live,
    )
    .map_err(map_err)?;
    Ok(crate::companion_history::merge(&state, local, input.range_start_ms, input.range_end_ms).await)
}

#[tauri::command]
pub fn list_alert_rules(state: State<AppState>) -> Result<Vec<AlertRule>, String> {
    let store = state.metrics.lock();
    list_rules(store.connection()).map_err(map_err)
}

#[tauri::command]
pub fn upsert_alert_rule(state: State<AppState>, rule: AlertRule) -> Result<AlertRule, String> {
    crate::license::require_write()?;
    let store = state.metrics.lock();
    upsert_rule(store.connection(), &rule).map_err(map_err)?;
    Ok(rule)
}

#[tauri::command]
pub fn delete_alert_rule(state: State<AppState>, id: String) -> Result<(), String> {
    crate::license::require_write()?;
    let store = state.metrics.lock();
    delete_rule(store.connection(), &id).map_err(map_err)
}

#[tauri::command]
pub fn list_alert_history(
    state: State<AppState>,
    server_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<AlertEventRecord>, String> {
    let store = state.metrics.lock();
    AlertHistoryStore::list_history(
        store.connection(),
        server_id.as_deref(),
        limit.unwrap_or(100),
    )
    .map_err(map_err)
}
