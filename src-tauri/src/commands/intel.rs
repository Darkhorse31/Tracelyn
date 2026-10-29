use crate::alerts::{self, AlertRule};
use crate::config::{RedisTargetConfig, ServerRecord};
use crate::intel::logic::replay_bands;
use crate::intel::notify::{self, build_jobs};
use crate::intel::report::{self, contains_secret_key, ReportDoc};
use crate::intel::store;
use crate::intel::types::{
    BandPoint, Channel, ChannelConfig, DigestSpec, IncidentDetail, IntelRule, SavedView, CHANNEL_KINDS,
    RULE_KINDS,
};
use crate::intel::{self, deliver_now, now_ms};
use crate::state::AppState;
use crate::webserver::RouteRule;
use base64::{engine::general_purpose::STANDARD, Engine};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, State};

fn err(value: impl ToString) -> String {
    value.to_string()
}

fn db_migrate(state: &AppState) -> Result<(), String> {
    let db = state.metrics.lock();
    intel::migrate(db.connection()).map_err(err)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelWrite {
    pub id: Option<String>,
    pub name: String,
    pub kind: String,
    pub enabled: bool,
    pub config: ChannelConfig,
    pub secret: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalOut {
    pub fired: u32,
    pub cleared: u32,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOut {
    pub filename: String,
    pub mime: String,
    pub base64: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub servers: usize,
    pub alert_rules: usize,
    pub intel_rules: usize,
    pub channels: usize,
    pub saved_views: usize,
    pub digests: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportQuery {
    pub format: String,
    pub incident_id: Option<String>,
    pub server_id: Option<String>,
    pub metric_key: Option<String>,
    pub range_start_ms: Option<i64>,
    pub range_end_ms: Option<i64>,
}

#[tauri::command]
pub fn intel_list_rules(state: State<AppState>) -> Result<Vec<IntelRule>, String> {
    db_migrate(&state)?;
    let db = state.metrics.lock();
    store::seed_defaults(db.connection(), now_ms()).map_err(err)?;
    store::list_rules(db.connection()).map_err(err)
}

#[tauri::command]
pub fn intel_upsert_rule(state: State<AppState>, rule: IntelRule) -> Result<IntelRule, String> {
    crate::license::require_write()?;
    db_migrate(&state)?;
    let mut rule = rule;
    validate_rule(&rule)?;
    if rule.id.trim().is_empty() {
        rule.id = store::new_id();
    }
    let now = now_ms();
    if rule.created_at_ms == 0 {
        rule.created_at_ms = now;
    }
    rule.updated_at_ms = now;
    rule.name = rule.name.trim().to_string();
    let db = state.metrics.lock();
    store::upsert_rule(db.connection(), &rule).map_err(err)?;
    Ok(rule)
}

#[tauri::command]
pub fn intel_delete_rule(state: State<AppState>, id: String) -> Result<(), String> {
    crate::license::require_write()?;
    db_migrate(&state)?;
    let db = state.metrics.lock();
    store::delete_rule(db.connection(), &id).map_err(err)
}

#[tauri::command]
pub fn intel_evaluate(app: AppHandle, state: State<AppState>, server_id: String) -> Result<EvalOut, String> {
    crate::license::require_write()?;
    db_migrate(&state)?;
    let probes = stale_probes(&state, &server_id)?;
    let out = intel::run_for_server(&state, &server_id, &probes)?;
    let mut errors = out.errors;
    if !out.notices.is_empty() {
        let channels = {
            let db = state.metrics.lock();
            store::list_channels(db.connection()).map_err(err)?
        };
        for notice in &out.notices {
            let (jobs, build_errors) = build_jobs(&channels, &notice.title, &notice.body, &notice.path);
            errors.extend(build_errors);
            errors.extend(deliver_now(&app, &jobs));
        }
    }
    Ok(EvalOut {
        fired: out.fired,
        cleared: out.cleared,
        errors,
    })
}

fn stale_probes(state: &AppState, server_id: &str) -> Result<HashMap<String, Result<f64, String>>, String> {
    let now = now_ms();
    let todo = {
        let db = state.metrics.lock();
        let rules = store::list_rules(db.connection()).map_err(err)?;
        let mut todo = Vec::new();
        for rule in rules.into_iter().filter(|rule| {
            rule.enabled
                && matches!(rule.kind.as_str(), "ssl_expiry" | "uptime")
                && rule.server_id.as_ref().map(|id| id == server_id).unwrap_or(true)
        }) {
            let runtime = store::load_state(db.connection(), &rule.id, server_id).map_err(err)?;
            if now.saturating_sub(runtime.checked_at_ms) < 300_000 && runtime.last_value.is_some() {
                continue;
            }
            todo.push(rule);
        }
        todo
    };
    let mut probes = HashMap::new();
    for rule in todo {
        let result = if rule.kind == "ssl_expiry" {
            intel::checks::probe_ssl_days(&rule.params.host, rule.params.port, now)
        } else {
            intel::checks::probe_tcp(&rule.params.host, rule.params.port)
        };
        probes.insert(rule.id, result);
    }
    Ok(probes)
}

#[tauri::command]
pub fn intel_list_channels(state: State<AppState>) -> Result<Vec<Channel>, String> {
    db_migrate(&state)?;
    let db = state.metrics.lock();
    let mut channels = store::list_channels(db.connection()).map_err(err)?;
    for channel in &mut channels {
        channel.has_secret = channel.kind == "windows" || notify::has_channel_secret(&channel.id);
    }
    Ok(channels)
}

#[tauri::command]
pub fn intel_upsert_channel(state: State<AppState>, input: ChannelWrite) -> Result<Channel, String> {
    crate::license::require_write()?;
    db_migrate(&state)?;
    validate_channel(&input)?;
    let id = input.id.filter(|value| !value.trim().is_empty()).unwrap_or_else(store::new_id);
    let channel = Channel {
        id: id.clone(),
        name: input.name.trim().to_string(),
        kind: input.kind,
        enabled: input.enabled,
        config: input.config,
        has_secret: false,
        created_at_ms: now_ms(),
    };
    if let Some(secret) = input.secret.filter(|value| !value.is_empty()) {
        notify::set_channel_secret(&id, &secret)?;
    }
    let db = state.metrics.lock();
    store::upsert_channel(db.connection(), &channel).map_err(err)?;
    drop(db);
    let mut saved = channel;
    saved.has_secret = saved.kind == "windows" || notify::has_channel_secret(&saved.id);
    Ok(saved)
}

#[tauri::command]
pub fn intel_delete_channel(state: State<AppState>, id: String) -> Result<(), String> {
    crate::license::require_write()?;
    db_migrate(&state)?;
    notify::delete_channel_secret(&id);
    let db = state.metrics.lock();
    store::delete_channel(db.connection(), &id).map_err(err)
}

#[tauri::command]
pub fn intel_test_channel(app: AppHandle, state: State<AppState>, id: String) -> Result<String, String> {
    crate::license::require_write()?;
    db_migrate(&state)?;
    let channel = {
        let db = state.metrics.lock();
        store::list_channels(db.connection())
            .map_err(err)?
            .into_iter()
            .find(|channel| channel.id == id)
            .ok_or_else(|| "channel not found".to_string())?
    };
    let title = "Tracelyn test";
    let body = format!(
        "Test notification from channel {}. If you can read this, delivery works.",
        channel.name
    );
    let (jobs, mut errors) = build_jobs(std::slice::from_ref(&channel), title, &body, "/alerts");
    if jobs.is_empty() {
        return Err(errors.join(". "));
    }
    errors.extend(deliver_now(&app, &jobs));
    if errors.is_empty() {
        Ok(format!("Sent a test through {}", channel.name))
    } else {
        Err(errors.join(". "))
    }
}

#[tauri::command]
pub fn intel_list_incidents(state: State<AppState>, server_id: Option<String>) -> Result<Vec<crate::intel::types::Incident>, String> {
    db_migrate(&state)?;
    let db = state.metrics.lock();
    store::list_incidents(db.connection(), server_id.as_deref()).map_err(err)
}

#[tauri::command]
pub fn intel_get_incident(state: State<AppState>, id: String) -> Result<IncidentDetail, String> {
    db_migrate(&state)?;
    let db = state.metrics.lock();
    let incident = store::get_incident(db.connection(), &id)
        .map_err(err)?
        .ok_or_else(|| "incident not found".to_string())?;
    let mut timeline = store::list_timeline(db.connection(), &id).map_err(err)?;
    let end = incident.resolved_at_ms.unwrap_or_else(now_ms);
    timeline.extend(
        store::list_feed_between(db.connection(), &incident.server_id, incident.opened_at_ms, end).map_err(err)?,
    );
    timeline.extend(
        store::anomalies_as_timeline(db.connection(), &incident.server_id, incident.opened_at_ms, end).map_err(err)?,
    );
    timeline.sort_by_key(|entry| (entry.ts_ms, entry.kind.clone()));
    timeline.truncate(500);
    Ok(IncidentDetail { incident, timeline })
}

#[tauri::command]
pub fn intel_ack_incident(state: State<AppState>, id: String) -> Result<(), String> {
    set_status(&state, &id, "ack", "Acknowledged")
}

#[tauri::command]
pub fn intel_resolve_incident(state: State<AppState>, id: String) -> Result<(), String> {
    set_status(&state, &id, "resolved", "Resolved")
}

#[tauri::command]
pub fn intel_snooze_incident(state: State<AppState>, id: String, minutes: u64) -> Result<(), String> {
    if !matches!(minutes, 15 | 60 | 1440) {
        return Err("Snooze for 15 minutes, 1 hour, or 1 day".into());
    }
    db_migrate(&state)?;
    let until = now_ms() + minutes as i64 * 60_000;
    let db = state.metrics.lock();
    store::snooze_incident(db.connection(), &id, until).map_err(err)?;
    store::add_timeline(db.connection(), &id, now_ms(), "audit", &format!("Snoozed for {minutes} minutes")).map_err(err)?;
    Ok(())
}

#[tauri::command]
pub fn intel_unread_count(state: State<AppState>) -> Result<i64, String> {
    db_migrate(&state)?;
    let db = state.metrics.lock();
    store::seed_defaults(db.connection(), now_ms()).map_err(err)?;
    store::unread_count(db.connection()).map_err(err)
}

#[tauri::command]
pub fn intel_mark_alerts_read(state: State<AppState>) -> Result<(), String> {
    db_migrate(&state)?;
    let db = state.metrics.lock();
    store::mark_alerts_read(db.connection()).map_err(err)
}

#[tauri::command]
pub fn intel_sound_enabled(state: State<AppState>) -> Result<bool, String> {
    db_migrate(&state)?;
    let db = state.metrics.lock();
    store::sound_enabled(db.connection()).map_err(err)
}

#[tauri::command]
pub fn intel_set_sound(state: State<AppState>, enabled: bool) -> Result<(), String> {
    db_migrate(&state)?;
    let db = state.metrics.lock();
    store::meta_set(db.connection(), "sound", if enabled { "1" } else { "0" }).map_err(err)
}

fn set_status(state: &AppState, id: &str, status: &str, message: &str) -> Result<(), String> {
    db_migrate(state)?;
    let now = now_ms();
    let db = state.metrics.lock();
    let incident = store::get_incident(db.connection(), id)
        .map_err(err)?
        .ok_or_else(|| "incident not found".to_string())?;
    if incident.status == "resolved" {
        return Err("incident is already resolved".into());
    }
    store::set_incident_status(db.connection(), id, status, now).map_err(err)?;
    store::add_timeline(db.connection(), id, now, "audit", message).map_err(err)?;
    Ok(())
}

#[tauri::command]
pub fn intel_add_incident_note(state: State<AppState>, id: String, text: String) -> Result<(), String> {
    db_migrate(&state)?;
    let text = text.trim();
    if text.is_empty() {
        return Err("note is empty".into());
    }
    if text.chars().count() > 4000 {
        return Err("note is too long".into());
    }
    let now = now_ms();
    let db = state.metrics.lock();
    let incident = store::get_incident(db.connection(), &id)
        .map_err(err)?
        .ok_or_else(|| "incident not found".to_string())?;
    store::add_timeline(db.connection(), &incident.id, now, "note", text).map_err(err)?;
    store::touch_incident(db.connection(), &incident.id, now).map_err(err)?;
    Ok(())
}

#[tauri::command]
pub fn intel_anomaly_series(
    state: State<AppState>,
    server_id: String,
    metric_key: String,
    range_start_ms: i64,
    range_end_ms: i64,
) -> Result<Vec<BandPoint>, String> {
    db_migrate(&state)?;
    if metric_key.len() > 120 || metric_key.chars().any(|c| c.is_control()) {
        return Err("metric key is not valid".into());
    }
    let db = state.metrics.lock();
    let points = store::metric_series(db.connection(), &server_id, &metric_key, range_start_ms, range_end_ms)
        .map_err(err)?;
    Ok(replay_bands(&points))
}

#[tauri::command]
pub fn intel_list_anomalies(
    state: State<AppState>,
    server_id: String,
    range_start_ms: i64,
    range_end_ms: i64,
) -> Result<Vec<crate::intel::types::AnomalyEvent>, String> {
    db_migrate(&state)?;
    let db = state.metrics.lock();
    store::list_anomalies(db.connection(), &server_id, range_start_ms, range_end_ms).map_err(err)
}

#[tauri::command]
pub fn intel_export_report(state: State<AppState>, query: ReportQuery) -> Result<FileOut, String> {
    db_migrate(&state)?;
    let format = query.format.as_str();
    if !matches!(format, "html" | "pdf") {
        return Err("format must be html or pdf".into());
    }
    let doc = if let Some(id) = query.incident_id.filter(|value| !value.is_empty()) {
        incident_doc(&state, &id)?
    } else {
        let server_id = query.server_id.ok_or_else(|| "choose a server or an incident".to_string())?;
        let metric = query.metric_key.unwrap_or_else(|| "cpu.percent".into());
        let end = query.range_end_ms.unwrap_or_else(now_ms);
        let start = query.range_start_ms.unwrap_or(end - 3_600_000);
        dashboard_doc(&state, &server_id, &metric, start, end)?
    };
    let (bytes, mime, ext) = if format == "pdf" {
        (report::render_pdf(&doc), "application/pdf", "pdf")
    } else {
        (report::render_html(&doc).into_bytes(), "text/html", "html")
    };
    let stem = doc.title.chars().take(40).filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>();
    let stem = if stem.is_empty() { "tracelyn-report".into() } else { stem };
    Ok(FileOut {
        filename: format!("{stem}.{ext}"),
        mime: mime.into(),
        base64: STANDARD.encode(bytes),
    })
}

fn incident_doc(state: &AppState, id: &str) -> Result<ReportDoc, String> {
    let db = state.metrics.lock();
    let incident = store::get_incident(db.connection(), id)
        .map_err(err)?
        .ok_or_else(|| "incident not found".to_string())?;
    let timeline = store::list_timeline(db.connection(), id).map_err(err)?;
    let server_name = state
        .config
        .lock()
        .get_server(&incident.server_id)
        .map(|server| server.name.clone())
        .unwrap_or(incident.server_id.clone());
    Ok(ReportDoc {
        title: incident.title.clone(),
        subtitle: format!(
            "{server_name} · {} · {} · opened {}",
            incident.severity,
            incident.status,
            report::format_utc(incident.opened_at_ms)
        ),
        rows: timeline
            .iter()
            .map(|entry| format!("{} {} — {}", report::format_utc(entry.ts_ms), entry.kind, entry.message))
            .collect(),
        series_name: String::new(),
        series: Vec::new(),
    })
}

fn dashboard_doc(
    state: &AppState,
    server_id: &str,
    metric_key: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<ReportDoc, String> {
    let server_name = state
        .config
        .lock()
        .get_server(server_id)
        .map(|server| server.name.clone())
        .unwrap_or_else(|_| server_id.to_string());
    let db = state.metrics.lock();
    let series = store::metric_series(db.connection(), server_id, metric_key, start_ms, end_ms).map_err(err)?;
    let bands = replay_bands(&series);
    let anomalies = bands.iter().filter(|point| point.anomalous).count();
    let rows = bands
        .iter()
        .filter(|point| point.anomalous)
        .take(40)
        .map(|point| {
            format!(
                "{} anomaly: {:.2} vs baseline {:.2} (z {:.1})",
                report::format_utc(point.ts_ms),
                point.value,
                point.baseline,
                point.z
            )
        })
        .collect::<Vec<_>>();
    Ok(ReportDoc {
        title: format!("{server_name} · {metric_key}"),
        subtitle: format!(
            "{} to {} · {anomalies} anomalies in view",
            report::format_utc(start_ms),
            report::format_utc(end_ms)
        ),
        rows,
        series_name: metric_key.to_string(),
        series,
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigBundle {
    version: u32,
    exported_at_ms: i64,
    servers: Vec<ServerRecord>,
    #[serde(default)]
    route_rules: HashMap<String, Vec<RouteRule>>,
    #[serde(default)]
    redis_targets: HashMap<String, RedisTargetConfig>,
    alert_rules: Vec<AlertRule>,
    intel_rules: Vec<IntelRule>,
    channels: Vec<Channel>,
    saved_views: Vec<SavedView>,
    digests: Vec<DigestSpec>,
}

#[tauri::command]
pub fn intel_export_config(state: State<AppState>) -> Result<FileOut, String> {
    db_migrate(&state)?;
    let config = state.config.lock();
    let mut route_rules = HashMap::new();
    let mut redis_targets = HashMap::new();
    for server in config.list_servers() {
        let rules = config.route_rules(&server.id);
        if !rules.is_empty() {
            route_rules.insert(server.id.clone(), rules);
        }
        let target = config.redis_target(&server.id);
        let empty = target.port.is_none()
            && target.socket.as_ref().map(|s| s.is_empty()).unwrap_or(true)
            && target.docker_container.as_ref().map(|s| s.is_empty()).unwrap_or(true);
        if !empty {
            redis_targets.insert(server.id.clone(), target);
        }
    }
    let db = state.metrics.lock();
    let bundle = ConfigBundle {
        version: 1,
        exported_at_ms: now_ms(),
        servers: config.list_servers().to_vec(),
        route_rules,
        redis_targets,
        alert_rules: alerts::list_rules(db.connection()).map_err(err)?,
        intel_rules: store::list_rules(db.connection()).map_err(err)?,
        channels: store::list_channels(db.connection()).map_err(err)?,
        saved_views: store::list_views(db.connection()).map_err(err)?,
        digests: store::list_digests(db.connection()).map_err(err)?,
    };
    drop(db);
    drop(config);
    let value = serde_json::to_value(&bundle).map_err(err)?;
    if contains_secret_key(&value) {
        return Err("export was blocked because a secret field was present".into());
    }
    let json = serde_json::to_string_pretty(&value).map_err(err)?;
    Ok(FileOut {
        filename: "tracelyn-config.json".into(),
        mime: "application/json".into(),
        base64: STANDARD.encode(json.as_bytes()),
    })
}

#[tauri::command]
pub fn intel_import_config(state: State<AppState>, raw: String) -> Result<ImportSummary, String> {
    crate::license::require_write()?;
    if raw.len() > 2_000_000 {
        return Err("config file is too large".into());
    }
    let bundle: ConfigBundle = serde_json::from_str(&raw).map_err(|e| format!("config JSON: {e}"))?;
    if bundle.version != 1 {
        return Err("this file is not a Tracelyn config version 1".into());
    }
    if bundle.servers.len() > 500
        || bundle.alert_rules.len() > 500
        || bundle.intel_rules.len() > 500
        || bundle.channels.len() > 50
        || bundle.saved_views.len() > 200
        || bundle.digests.len() > 20
    {
        return Err("config file has more entries than Tracelyn will import".into());
    }
    for rule in &bundle.intel_rules {
        validate_rule(rule)?;
    }
    for channel in &bundle.channels {
        if !CHANNEL_KINDS.contains(&channel.kind.as_str()) {
            return Err(format!("unknown channel kind {}", channel.kind));
        }
    }
    db_migrate(&state)?;
    {
        let mut config = state.config.lock();
        for server in &bundle.servers {
            if server.name.trim().is_empty() || server.host.trim().is_empty() {
                return Err("a server in the file is missing a name or host".into());
            }
            config.upsert_server(server.clone());
        }
        for (server_id, rules) in &bundle.route_rules {
            config.set_route_rules(server_id, rules.clone()).map_err(err)?;
        }
        for (server_id, target) in &bundle.redis_targets {
            config.set_redis_target(server_id, target.clone()).map_err(err)?;
        }
        config.save().map_err(err)?;
    }
    let db = state.metrics.lock();
    for rule in &bundle.alert_rules {
        alerts::upsert_rule(db.connection(), rule).map_err(err)?;
    }
    for rule in &bundle.intel_rules {
        store::upsert_rule(db.connection(), rule).map_err(err)?;
    }
    for channel in &bundle.channels {
        let mut clean = channel.clone();
        clean.has_secret = false;
        store::upsert_channel(db.connection(), &clean).map_err(err)?;
    }
    for view in &bundle.saved_views {
        validate_view(view)?;
        store::upsert_view(db.connection(), view).map_err(err)?;
    }
    for digest in &bundle.digests {
        validate_digest(digest)?;
        store::upsert_digest(db.connection(), digest).map_err(err)?;
    }
    Ok(ImportSummary {
        servers: bundle.servers.len(),
        alert_rules: bundle.alert_rules.len(),
        intel_rules: bundle.intel_rules.len(),
        channels: bundle.channels.len(),
        saved_views: bundle.saved_views.len(),
        digests: bundle.digests.len(),
    })
}

#[tauri::command]
pub fn intel_list_views(state: State<AppState>) -> Result<Vec<SavedView>, String> {
    db_migrate(&state)?;
    let db = state.metrics.lock();
    store::list_views(db.connection()).map_err(err)
}

#[tauri::command]
pub fn intel_upsert_view(state: State<AppState>, view: SavedView) -> Result<SavedView, String> {
    db_migrate(&state)?;
    let mut view = view;
    if view.id.trim().is_empty() {
        view.id = store::new_id();
    }
    validate_view(&view)?;
    let db = state.metrics.lock();
    store::upsert_view(db.connection(), &view).map_err(err)?;
    Ok(view)
}

#[tauri::command]
pub fn intel_delete_view(state: State<AppState>, id: String) -> Result<(), String> {
    db_migrate(&state)?;
    let db = state.metrics.lock();
    store::delete_view(db.connection(), &id).map_err(err)
}

#[tauri::command]
pub fn intel_list_digests(state: State<AppState>) -> Result<Vec<DigestSpec>, String> {
    db_migrate(&state)?;
    let db = state.metrics.lock();
    store::list_digests(db.connection()).map_err(err)
}

#[tauri::command]
pub fn intel_upsert_digest(state: State<AppState>, digest: DigestSpec) -> Result<DigestSpec, String> {
    crate::license::require_write()?;
    db_migrate(&state)?;
    let mut digest = digest;
    if digest.id.trim().is_empty() {
        digest.id = store::new_id();
    }
    validate_digest(&digest)?;
    let db = state.metrics.lock();
    store::upsert_digest(db.connection(), &digest).map_err(err)?;
    Ok(digest)
}

#[tauri::command]
pub fn intel_delete_digest(state: State<AppState>, id: String) -> Result<(), String> {
    crate::license::require_write()?;
    db_migrate(&state)?;
    let db = state.metrics.lock();
    store::delete_digest(db.connection(), &id).map_err(err)
}

#[tauri::command]
pub fn intel_send_digest(app: AppHandle, state: State<AppState>, id: String) -> Result<String, String> {
    crate::license::require_write()?;
    db_migrate(&state)?;
    let now = now_ms();
    let (digest, subject, body, channel) = {
        let db = state.metrics.lock();
        let mut digest = store::list_digests(db.connection())
            .map_err(err)?
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| "digest not found".to_string())?;
        let since = if digest.cadence == "weekly" {
            now - 7 * 86_400_000
        } else {
            now - 86_400_000
        };
        let incidents = store::incidents_since(db.connection(), since).map_err(err)?;
        let (_, anomaly_count) = store::incident_counts(db.connection(), since).map_err(err)?;
        let (subject, body) = report::digest_message(&digest, &incidents, anomaly_count, since);
        let channel = store::list_channels(db.connection())
            .map_err(err)?
            .into_iter()
            .find(|item| item.id == digest.channel_id)
            .ok_or_else(|| "the digest channel was deleted".to_string())?;
        digest.last_attempt_ms = Some(now);
        store::upsert_digest(db.connection(), &digest).map_err(err)?;
        (digest, subject, body, channel)
    };
    let (jobs, mut errors) = build_jobs(std::slice::from_ref(&channel), &subject, &body, "/alerts");
    errors.extend(deliver_now(&app, &jobs));
    let db = state.metrics.lock();
    let mut digest = digest;
    if errors.is_empty() {
        digest.last_sent_ms = Some(now);
        digest.last_error = None;
        store::upsert_digest(db.connection(), &digest).map_err(err)?;
        Ok(format!("Sent “{}”", digest.name))
    } else {
        digest.last_error = Some(errors.join(". "));
        store::upsert_digest(db.connection(), &digest).map_err(err)?;
        Err(errors.join(". "))
    }
}

fn validate_rule(rule: &IntelRule) -> Result<(), String> {
    if rule.name.trim().is_empty() || rule.name.chars().count() > 200 {
        return Err("rule name is required".into());
    }
    if !RULE_KINDS.contains(&rule.kind.as_str()) {
        return Err("unknown rule kind".into());
    }
    if !matches!(rule.severity.as_str(), "info" | "warning" | "critical") {
        return Err("severity must be info, warning, or critical".into());
    }
    if !rule.params.threshold.is_finite() {
        return Err("threshold must be a finite number".into());
    }
    if let Some(clear) = rule.params.clear_threshold {
        if !clear.is_finite() {
            return Err("clear threshold must be a finite number".into());
        }
    }
    if rule.params.duration_secs > 7 * 86_400 || rule.params.clear_duration_secs > 7 * 86_400 {
        return Err("duration is too long".into());
    }
    if rule.params.cooldown_secs > 7 * 86_400 {
        return Err("cooldown is too long".into());
    }
    if let (Some(start), Some(end)) = (rule.params.quiet_start_min, rule.params.quiet_end_min) {
        if start > 1439 || end > 1439 {
            return Err("quiet hours use minutes from midnight, from 0 to 1439".into());
        }
    }
    if rule.params.app_id.chars().count() > 80 {
        return Err("app id is too long".into());
    }
    if rule.kind == "log_pattern" {
        if rule.params.pattern.is_empty() || rule.params.pattern.len() > 200 {
            return Err("log pattern must be 1–200 characters".into());
        }
        Regex::new(&rule.params.pattern).map_err(|e| format!("pattern: {e}"))?;
    }
    if matches!(rule.kind.as_str(), "ssl_expiry" | "uptime") {
        let http=rule.kind=="uptime" && (rule.params.host.starts_with("https://")||rule.params.host.starts_with("http://"));
        if http {
            let url=url::Url::parse(&rule.params.host).map_err(|_|"Invalid HTTP check URL")?;
            if url.host_str().is_none()||!url.username().is_empty()||url.password().is_some(){return Err("Use an HTTP URL without embedded credentials".into());}
        } else {notify::validate_host(&rule.params.host)?;}
        if rule.kind == "uptime" && !http && rule.params.port == 0 {
            return Err("uptime check needs a port".into());
        }
    }
    if rule.kind == "metric" && rule.params.metric_key.trim().is_empty() {
        return Err("choose a metric key".into());
    }
    Ok(())
}

fn validate_channel(input: &ChannelWrite) -> Result<(), String> {
    if input.name.trim().is_empty() || input.name.chars().count() > 120 {
        return Err("channel name is required".into());
    }
    if !CHANNEL_KINDS.contains(&input.kind.as_str()) {
        return Err("unknown channel kind".into());
    }
    match input.kind.as_str() {
        "telegram" if input.config.chat_id.trim().is_empty() => {
            Err("Telegram needs a chat id".into())
        }
        "email" => {
            notify::validate_host(&input.config.smtp_host)?;
            notify::validate_mail(&input.config.mail_from)?;
            notify::validate_mail(&input.config.mail_to)?;
            if input.config.smtp_port == 0 {
                return Err("SMTP port is required".into());
            }
            if !matches!(input.config.smtp_tls.as_str(), "starttls" | "implicit" | "none") {
                return Err("SMTP security must be starttls, implicit, or none".into());
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_view(view: &SavedView) -> Result<(), String> {
    if view.name.trim().is_empty() || view.name.chars().count() > 120 {
        return Err("view name is required".into());
    }
    if !view.path.starts_with('/') || view.path.chars().count() > 300 || view.path.chars().any(|c| c.is_control()) {
        return Err("view path must start with /".into());
    }
    if view.note.chars().count() > 2000 {
        return Err("view note is too long".into());
    }
    Ok(())
}

fn validate_digest(digest: &DigestSpec) -> Result<(), String> {
    if digest.name.trim().is_empty() {
        return Err("digest name is required".into());
    }
    if !matches!(digest.cadence.as_str(), "daily" | "weekly") {
        return Err("cadence must be daily or weekly".into());
    }
    if digest.hour_utc > 23 || digest.weekday > 6 {
        return Err("hour must be 0–23 and weekday 0–6".into());
    }
    if digest.channel_id.trim().is_empty() {
        return Err("choose a channel".into());
    }
    Ok(())
}
