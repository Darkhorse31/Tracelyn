mod capacity;
mod cron;
mod local;
pub mod model;
mod nginx;
mod parsers;
mod presence;
pub mod remote;
mod scripts;
mod store;
#[cfg(test)]
mod tests;

use crate::{
    diagnostics::{DiagnosticEntry, DiagnosticStatus},
    state::{AppState, ConnectionStatus},
};
use model::*;
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashMap, HashSet};
use tauri::{AppHandle, Manager, State};

pub struct InfraState {
    snapshots: Mutex<HashMap<String, BTreeMap<String, Snapshot>>>,
    generations: Mutex<HashMap<String, u64>>,
    local_slots: std::sync::Arc<tokio::sync::Semaphore>,
}
impl Default for InfraState {
    fn default() -> Self {
        Self {
            snapshots: Mutex::default(),
            generations: Mutex::default(),
            local_slots: std::sync::Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }
}
impl InfraState {
    pub fn put(&self, server: &str, mut snapshot: Snapshot) {
        if snapshot.rows.len() > 1000 {
            snapshot.rows.truncate(1000);
            snapshot.phase = Phase::Partial;
            snapshot.message =
                "Showing the first 1,000 resources. The response limit was reached.".into();
        }
        self.snapshots
            .lock()
            .entry(server.into())
            .or_default()
            .insert(snapshot.collector.clone(), snapshot);
    }
    pub fn has_collector(&self, server: &str, collector: &str) -> bool {
        self.snapshots
            .lock()
            .get(server)
            .and_then(|map| map.get(collector))
            .is_some()
    }
    pub fn minimum_number(&self, server: &str, collector: &str, field: &str) -> Option<f64> {
        self.snapshots.lock().get(server).and_then(|map| map.get(collector)).and_then(|snap| {
            snap.rows.iter().filter_map(|row| row.number(field)).reduce(f64::min)
        })
    }
    pub fn maximum_number(&self, server: &str, collector: &str, field: &str) -> Option<f64> {
        self.snapshots.lock().get(server).and_then(|map| map.get(collector)).and_then(|snap| {
            snap.rows.iter().filter_map(|row| row.number(field)).reduce(f64::max)
        })
    }
    pub fn field_strings(&self, server: &str, collector: &str, field: &str) -> Vec<String> {
        self.snapshots
            .lock()
            .get(server)
            .and_then(|map| map.get(collector))
            .map(|snap| {
                snap.rows
                    .iter()
                    .filter_map(|row| row.fields.get(field).and_then(|value| value.as_str()).map(|s| s.to_string()))
                    .take(100)
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn uptime_ok(&self, server: &str) -> Option<bool> {
        let snaps = self.snapshots.lock();
        let snap = snaps.get(server)?.get("uptime")?;
        if snap.rows.is_empty() {
            return None;
        }
        Some(!snap.rows.iter().any(|row| row.state == "down"))
    }
    pub fn generation(&self, server: &str) -> u64 {
        *self.generations.lock().get(server).unwrap_or(&0)
    }
    pub fn put_certificate(&self, server: &str, mut row: Resource) {
        row.set("checkedAtMs", crate::metrics_ingest::now_ms());
        let mut snapshots = self.snapshots.lock();
        let snapshot = snapshots
            .entry(server.into())
            .or_default()
            .entry("certificates_pc".into())
            .or_insert_with(|| Snapshot::ok("certificates_pc", vec![]));
        if let Some(existing) = snapshot.rows.iter_mut().find(|r| r.id == row.id) {
            *existing = row;
        } else if snapshot.rows.len() < 2048 {
            snapshot.rows.push(row);
        }
        snapshot.sampled_at_ms = crate::metrics_ingest::now_ms();
        snapshot.message = format!(
            "{} public certificates inspected from this PC · refreshed every 6 hours",
            snapshot.rows.len()
        );
        snapshot.phase = if snapshot
            .rows
            .iter()
            .any(|r| ["error", "unknown", "critical"].contains(&r.state.as_str()))
        {
            Phase::Partial
        } else {
            Phase::Ok
        };
        snapshot.fix_hint = if snapshot.phase == Phase::Partial {
            Some(
                "Check the affected domain's DNS, HTTPS listener and certificate trust chain."
                    .into(),
            )
        } else {
            None
        };
    }
    pub fn prune(&self, servers: &HashSet<String>) {
        self.snapshots.lock().retain(|id, _| servers.contains(id));
        self.generations.lock().retain(|id, _| servers.contains(id));
    }
    pub fn sync_local(&self, server: &str, checks: &[UptimeCheck], domains: &[String]) {
        let mut all = self.snapshots.lock();
        let map = all.entry(server.into()).or_default();
        let snapshot = map
            .entry("uptime".into())
            .or_insert_with(|| Snapshot::ok("uptime", vec![]));
        snapshot
            .rows
            .retain(|r| checks.iter().any(|c| c.id == r.id));
        snapshot.message = format!(
            "{} configured PC checks · {} enabled · {} results available",
            checks.len(),
            checks.iter().filter(|c| c.enabled).count(),
            snapshot.rows.len()
        );
        if let Some(certs) = map.get_mut("certificates_pc") {
            certs.rows.retain(|r| domains.contains(&r.id));
        }
    }
    pub fn put_uptime(&self, server: &str, row: Resource, saved: bool) {
        let mut all = self.snapshots.lock();
        let snapshot = all
            .entry(server.into())
            .or_default()
            .entry("uptime".into())
            .or_insert_with(|| Snapshot::ok("uptime", vec![]));
        if let Some(old) = snapshot.rows.iter_mut().find(|r| r.id == row.id) {
            *old = row;
        } else if snapshot.rows.len() < 32 {
            snapshot.rows.push(row);
        }
        snapshot.sampled_at_ms = crate::metrics_ingest::now_ms();
        snapshot.phase = if saved { Phase::Ok } else { Phase::Error };
        snapshot.fix_hint = if saved {
            None
        } else {
            Some("Check local disk space and SQLite access.".into())
        };
    }
    pub fn diagnostics(&self, server: &str) -> Vec<DiagnosticEntry> {
        let snapshots = self.snapshots.lock();
        let map = snapshots.get(server);
        COLLECTORS.iter().map(|name| {
            let snapshot=map.and_then(|m|m.get(*name));
            DiagnosticEntry{collector:format!("infra:{name}"),status:match snapshot.map(|s|s.phase) {Some(Phase::Ok)=>DiagnosticStatus::Ok,Some(Phase::Error|Phase::PermissionDenied|Phase::TimedOut)=>DiagnosticStatus::Error,_=>DiagnosticStatus::Warning},message:snapshot.map(|s|format!("{:?}: {}",s.phase,s.message)).unwrap_or_else(||"Waiting for the first infrastructure sample".into()),fix_hint:snapshot.and_then(|s|s.fix_hint.clone()).or_else(||if snapshot.is_none(){Some("Connect the server. PC checks start after a domain is discovered or a check is added.".into())}else{None})}
        }).collect()
    }
    pub fn collector_phase(&self, server: &str, collector: &str) -> Option<String> {
        let snapshots = self.snapshots.lock();
        let phase = snapshots.get(server)?.get(collector)?.phase;
        Some(
            match phase {
                Phase::Loading => "loading",
                Phase::Ok => "ok",
                Phase::Partial => "partial",
                Phase::NotInstalled => "not_installed",
                Phase::AuthNeeded => "auth_needed",
                Phase::PermissionDenied => "permission_denied",
                Phase::TimedOut => "timed_out",
                Phase::Error => "error",
                Phase::InstalledStopped => "installed_stopped",
                Phase::NotConfigured => "not_configured",
            }
            .into(),
        )
    }
    pub fn engine_configured(&self, db: &rusqlite::Connection, server: &str, engine: &str) -> bool {
        let _ = self;
        let target = store::target(db, server, engine);
        let creds = credentials(server, engine).unwrap_or_default();
        !creds.username.is_empty()
            || !creds.password.is_empty()
            || !target.host.is_empty()
            || target.port.is_some()
            || !target.container.is_empty()
            || !target.database.is_empty()
    }
}
pub fn setup(app: AppHandle) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let db = state.metrics.lock();
        store::migrate(db.connection()).map_err(|e| e.to_string())?;
    }
    local::spawn(app);
    Ok(())
}
pub fn remove_server_data(state: &AppState, server: &str) -> Result<(), String> {
    for engine in ["mysql", "postgres", "mongo", "rabbitmq"] {
        match entry(server, engine)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => {
                return Err(
                    "Could not remove infrastructure credentials from Windows Credential Manager"
                        .into(),
                )
            }
        }
    }
    let db = state.metrics.lock();
    let tx = db
        .connection()
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM infra_check_results WHERE check_id IN (SELECT id FROM infra_checks WHERE server_id=?1)",[server]).map_err(|e|e.to_string())?;
    for table in [
        "infra_checks",
        "infra_settings",
        "infra_domains",
        "infra_disk",
    ] {
        tx.execute(&format!("DELETE FROM {table} WHERE server_id=?1"), [server])
            .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
fn ensure_server(state: &AppState, server: &str) -> Result<(), String> {
    if state
        .config
        .lock()
        .list_servers()
        .iter()
        .any(|s| s.id == server)
    {
        Ok(())
    } else {
        Err("Server no longer exists. Select a configured server.".into())
    }
}
fn entry(server: &str, engine: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("Tracelyn", &format!("server:{server}:infra:{engine}"))
        .map_err(|e| e.to_string())
}
fn credentials(server: &str, engine: &str) -> Result<Credentials, String> {
    match entry(server, engine)?.get_password() {
        Ok(value) => serde_json::from_str(&value)
            .map_err(|_| "Stored credentials are invalid. Save them again.".into()),
        Err(keyring::Error::NoEntry) => Ok(Credentials::default()),
        Err(_) => Err("Windows Credential Manager could not be read".into()),
    }
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetView {
    target: Target,
    has_credentials: bool,
}

#[tauri::command]
pub fn get_infra_snapshot(
    state: State<AppState>,
    infra: State<InfraState>,
    server_id: String,
    collectors: Option<Vec<String>>,
    include_history: Option<bool>,
    summary_only: Option<bool>,
) -> Result<Vec<Snapshot>, String> {
    ensure_server(&state, &server_id)?;
    let snapshots = infra.snapshots.lock();
    let map = snapshots.get(&server_id);
    let connected = matches!(
        state.runtime(&server_id).status,
        ConnectionStatus::Connected | ConnectionStatus::Degraded
    );
    Ok(COLLECTORS
        .iter()
        .filter(|name| {
            collectors
                .as_ref()
                .map(|ids| ids.iter().any(|id| id == **name))
                .unwrap_or(true)
        })
        .map(|name| {
            map.and_then(|m| m.get(*name))
                .map(|s| {
                    let rows = s
                        .rows
                        .iter()
                        .map(|r| {
                            let fields = if summary_only.unwrap_or(false) {
                                serde_json::Map::new()
                            } else {
                                r.fields
                                    .iter()
                                    .filter(|(key, _)| {
                                        include_history.unwrap_or(false)
                                            || key.as_str() != "history"
                                    })
                                    .map(|(k, v)| (k.clone(), v.clone()))
                                    .collect()
                            };
                            Resource {
                                id: r.id.clone(),
                                name: r.name.clone(),
                                state: r.state.clone(),
                                fields,
                            }
                        })
                        .collect();
                    Snapshot {
                        collector: s.collector.clone(),
                        phase: s.phase,
                        message: s.message.clone(),
                        fix_hint: s.fix_hint.clone(),
                        detail: s.detail.clone(),
                        sampled_at_ms: s.sampled_at_ms,
                        rows,
                    }
                })
                .unwrap_or_else(|| {
                    Snapshot::problem(
                        name,
                        Phase::Loading,
                        if connected {
                            "Waiting for the first collector pass"
                        } else {
                            "Connect the server to collect infrastructure metrics"
                        },
                        "Connect from Overview. PC checks run while this application is open.",
                    )
                })
        })
        .collect())
}
#[tauri::command]
pub fn refresh_infra(
    state: State<AppState>,
    infra: State<InfraState>,
    server_id: String,
) -> Result<(), String> {
    ensure_server(&state, &server_id)?;
    let mut generations = infra.generations.lock();
    let value = generations.entry(server_id).or_default();
    *value = value.wrapping_add(1);
    Ok(())
}
#[tauri::command]
pub fn get_infra_target(
    state: State<AppState>,
    server_id: String,
    engine: String,
) -> Result<TargetView, String> {
    ensure_server(&state, &server_id)?;
    let target = store::target(state.metrics.lock().connection(), &server_id, &engine);
    let credentials = credentials(&server_id, &engine)?;
    Ok(TargetView {
        target,
        has_credentials: !credentials.username.is_empty() || !credentials.password.is_empty(),
    })
}
#[tauri::command]
pub fn set_infra_target(
    state: State<AppState>,
    infra: State<InfraState>,
    server_id: String,
    engine: String,
    target: Target,
    credentials: Option<Credentials>,
) -> Result<(), String> {
    ensure_server(&state, &server_id)?;
    if !["mysql", "postgres", "mongo", "rabbitmq"].contains(&engine.as_str()) {
        return Err("Unsupported credential target".into());
    }
    if target.host.len() > 253
        || !target
            .host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".:-[]".contains(&c))
        || target.container.len() > 128
        || !target
            .container
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
        || target.database.len() > 63
        || !target
            .database
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        || target.port == Some(0)
    {
        return Err("Use a plain hostname, valid port, container name and database name.".into());
    }
    if let Some(credentials) = credentials {
        if credentials.username.len() > 256 || credentials.password.len() > 4096 {
            return Err("Credentials exceed the supported size".into());
        }
        let secret = entry(&server_id, &engine)?;
        if credentials.username.is_empty() && credentials.password.is_empty() {
            match secret.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => {}
                Err(_) => return Err("Could not clear Windows Credential Manager entry".into()),
            }
        } else {
            secret
                .set_password(&serde_json::to_string(&credentials).map_err(|e| e.to_string())?)
                .map_err(|_| "Could not save to Windows Credential Manager")?;
        }
    }
    store::save_target(
        state.metrics.lock().connection(),
        &server_id,
        &engine,
        &target,
    )?;
    let mut generations = infra.generations.lock();
    let value = generations.entry(server_id).or_default();
    *value = value.wrapping_add(1);
    Ok(())
}
#[tauri::command]
pub fn list_infra_domains(
    state: State<AppState>,
    server_id: String,
) -> Result<Vec<String>, String> {
    ensure_server(&state, &server_id)?;
    store::domains(state.metrics.lock().connection(), &server_id).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn list_uptime_checks(
    state: State<AppState>,
    server_id: String,
) -> Result<Vec<CheckView>, String> {
    ensure_server(&state, &server_id)?;
    store::check_views(state.metrics.lock().connection(), &server_id).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn upsert_uptime_check(
    state: State<AppState>,
    mut check: UptimeCheck,
) -> Result<UptimeCheck, String> {
    crate::license::require_write()?;
    ensure_server(&state, &check.server_id)?;
    check.url = local::validate_url(&check.url)?.to_string();
    if !(15..=86400).contains(&check.interval_seconds) {
        return Err("Choose an interval from 15 to 86,400 seconds".into());
    }
    let db = state.metrics.lock();
    let existing = store::checks(db.connection(), None).map_err(|e| e.to_string())?;
    if check.id.is_empty() {
        if existing.len() >= 128
            || existing
                .iter()
                .filter(|c| c.server_id == check.server_id)
                .count()
                >= 32
        {
            return Err("Limit reached: 32 checks per server and 128 overall".into());
        }
        check.id = uuid::Uuid::new_v4().to_string();
    } else if !existing
        .iter()
        .any(|c| c.id == check.id && c.server_id == check.server_id)
    {
        return Err("Check no longer exists".into());
    }
    if existing
        .iter()
        .any(|c| c.id != check.id && c.server_id == check.server_id && c.url == check.url)
    {
        return Err("A check for this URL already exists".into());
    }
    db.connection().execute("INSERT INTO infra_checks VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET json=excluded.json",rusqlite::params![check.id,check.server_id,serde_json::to_string(&check).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
    Ok(check)
}
#[tauri::command]
pub fn delete_uptime_check(
    state: State<AppState>,
    server_id: String,
    check_id: String,
) -> Result<(), String> {
    crate::license::require_write()?;
    ensure_server(&state, &server_id)?;
    let db = state.metrics.lock();
    let tx = db
        .connection()
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    let count = tx
        .execute(
            "DELETE FROM infra_checks WHERE id=?1 AND server_id=?2",
            rusqlite::params![check_id, server_id],
        )
        .map_err(|e| e.to_string())?;
    if count > 0 {
        tx.execute(
            "DELETE FROM infra_check_results WHERE check_id=?1",
            [check_id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn run_uptime_check(
    app: AppHandle,
    server_id: String,
    check_id: String,
) -> Result<CheckResult, String> {
    let slots = app.state::<InfraState>().local_slots.clone();
    let _permit = slots
        .try_acquire_owned()
        .map_err(|_| "All four PC workers are busy. Retry shortly.")?;
    let check = {
        let state = app.state::<AppState>();
        ensure_server(&state, &server_id)?;
        let db = state.metrics.lock();
        store::checks(db.connection(), Some(&server_id))
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|c| c.id == check_id)
            .ok_or("Check no longer exists")?
    };
    let result = local::check_with(&check.url, check.allow_self_signed).await;
    {
        let state = app.state::<AppState>();
        let db = state.metrics.lock();
        if store::checks(db.connection(), Some(&server_id))
            .map_err(|e| e.to_string())?
            .iter()
            .any(|c| c.id == check_id)
        {
            store::save_result(db.connection(), &check_id, &result).map_err(|e| e.to_string())?;
        }
    }
    Ok(result)
}
