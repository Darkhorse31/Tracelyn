use crate::alerts::{
    AlertEngine, AlertEventRecord, AlertHistoryStore, AlertRule, EvaluationInput, EvaluationOutcome,
    delete_rule, list_rules, upsert_rule,
};
use crate::state::AppState;
use crate::storage::{query_metrics, run_maintenance, QueryResult, RawSample, RetentionConfig};
use serde::Deserialize;
use tauri::{AppHandle, State};
use tauri_plugin_notification::NotificationExt;

fn map_err(err: impl ToString) -> String {
    err.to_string()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricSampleInput {
    pub server_id: String,
    pub metric_key: String,
    pub ts_ms: i64,
    pub value: f64,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateAlertsInput {
    pub server_id: String,
    pub ts_ms: i64,
    pub connected: bool,
    pub metrics: Vec<MetricSampleInput>,
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
    let mut store = state.metrics.lock();
    store.set_retention(config).map_err(map_err)?;
    Ok(store.retention().clone())
}

#[tauri::command]
pub fn insert_metric_samples(
    state: State<AppState>,
    samples: Vec<MetricSampleInput>,
) -> Result<(), String> {
    let batch: Vec<RawSample> = samples
        .into_iter()
        .map(|s| RawSample {
            server_id: s.server_id,
            metric_key: s.metric_key,
            ts_ms: s.ts_ms,
            value: s.value,
        })
        .collect();
    state.metrics.lock().insert_raw_batch(&batch).map_err(map_err)
}

#[tauri::command]
pub fn query_metric_series(
    state: State<AppState>,
    input: QueryMetricsInput,
) -> Result<QueryResult, String> {
    query_metrics(
        &*state.metrics.lock(),
        &input.server_id,
        &input.metric_key,
        input.range_start_ms,
        input.range_end_ms,
        input.prefer_live,
    )
    .map_err(map_err)
}

#[tauri::command]
pub fn run_storage_jobs(state: State<AppState>) -> Result<(), String> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    run_maintenance(&*state.metrics.lock(), now_ms).map_err(map_err)
}

#[tauri::command]
pub fn list_alert_rules(state: State<AppState>) -> Result<Vec<AlertRule>, String> {
    let store = state.metrics.lock();
    list_rules(store.connection()).map_err(map_err)
}

#[tauri::command]
pub fn upsert_alert_rule(state: State<AppState>, rule: AlertRule) -> Result<AlertRule, String> {
    let store = state.metrics.lock();
    upsert_rule(store.connection(), &rule).map_err(map_err)?;
    Ok(rule)
}

#[tauri::command]
pub fn delete_alert_rule(state: State<AppState>, id: String) -> Result<(), String> {
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

#[tauri::command]
pub fn evaluate_alerts(
    app: AppHandle,
    state: State<AppState>,
    input: EvaluateAlertsInput,
) -> Result<EvaluationOutcome, String> {
    let metrics: std::collections::HashMap<String, f64> = input
        .metrics
        .into_iter()
        .map(|m| (m.metric_key, m.value))
        .collect();
    let eval_input = EvaluationInput {
        server_id: input.server_id,
        ts_ms: input.ts_ms,
        metrics,
        connected: input.connected,
    };
    let store = state.metrics.lock();
    let outcome = AlertEngine::evaluate(store.connection(), &eval_input).map_err(map_err)?;

    for transition in &outcome.transitions {
        if !transition.fired {
            continue;
        }
        let _ = app
            .notification()
            .builder()
            .title("Tracelyn alert")
            .body(&transition.message)
            .show();
    }

    Ok(outcome)
}
