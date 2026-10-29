use crate::apps::types::TracelynApp;
use crate::infra::InfraState;
use crate::secrets::{get_secret, SecretKind};
use crate::setup::{build_checks, probe_script, AskedPath, EngineFact, LocalFacts, SetupCheck};
use crate::ssh::key_file::{
    detected_private_keys, inspect_key_file, ssh_dir, KeyInspection, SshKeyList,
};
use crate::ssh::{connect_persistent, run_command};
use crate::state::AppState;
use std::path::Path;
use tauri::State;

#[tauri::command]
pub async fn run_setup_checks(
    state: State<'_, AppState>,
    infra: State<'_, InfraState>,
    server_id: String,
) -> Result<Vec<SetupCheck>, String> {
    let server = {
        let config = state.config.lock();
        config
            .get_server(&server_id)
            .map_err(|e| e.to_string())?
            .clone()
    };
    let username = server.username.clone();
    let mut local = gather_local(&state, &infra, &server_id);
    let paths = local.paths.clone();
    let script = probe_script(&paths);
    match connect_persistent(&server, state.host_keys.clone()).await {
        Ok(session) => match run_command(&session, &script).await {
            Ok(text) => {
                let probe = crate::setup::parse_probe(&text);
                Ok(build_checks(&username, Some(&probe), &local))
            }
            Err(err) => {
                local.ssh_error = Some(err.to_string());
                Ok(build_checks(&username, None, &local))
            }
        },
        Err(err) => {
            local.ssh_error = Some(err.to_string());
            Ok(build_checks(&username, None, &local))
        }
    }
}

fn gather_local(state: &AppState, infra: &InfraState, server_id: &str) -> LocalFacts {
    let apps = state.apps_for_server(server_id);
    let redis_target = state.config.lock().redis_target(server_id);
    let redis_configured = secret_present(server_id, SecretKind::RedisPassword)
        || secret_present(server_id, SecretKind::RedisUsername)
        || redis_target.port.is_some()
        || redis_target.socket.as_deref().is_some_and(|s| !s.is_empty())
        || redis_target
            .docker_container
            .as_deref()
            .is_some_and(|s| !s.is_empty());
    let db = state.metrics.lock();
    let conn = db.connection();
    let engines = [
        ("mysql", "MySQL"),
        ("postgres", "PostgreSQL"),
        ("mongo", "MongoDB"),
        ("rabbitmq", "RabbitMQ"),
    ]
        .into_iter()
        .map(|(id, title)| EngineFact {
            id: id.into(),
            title: title.into(),
            configured: infra.engine_configured(conn, server_id, id),
            phase: infra.collector_phase(server_id, id),
        })
        .collect();
    let otel = crate::insight::store::watched_files(conn, server_id).unwrap_or_default();
    let insight = crate::insight::store::setup(conn, server_id).ok();
    let otel_spans = insight.as_ref().map(|row| row.stats.spans_ingested).unwrap_or(0);
    let otel_detail = insight.as_ref().map(|row| row.stats.detail.clone()).unwrap_or_default();
    let otel_last_write = insight
        .as_ref()
        .and_then(|row| row.stats.last_write.clone())
        .unwrap_or_default();
    drop(db);
    let mut paths = Vec::new();
    let mut n = 0u32;
    for file in otel.into_iter().filter(|f| f.kind == "otel" && !f.is_dir) {
        paths.push(AskedPath {
            id: format!("p{n}"),
            group: "otel".into(),
            label: if file.app_id.is_empty() {
                "server".into()
            } else {
                file.app_id
            },
            path: file.path,
        });
        n += 1;
    }
    for (label, path) in app_log_paths(&apps) {
        paths.push(AskedPath {
            id: format!("p{n}"),
            group: "app".into(),
            label,
            path,
        });
        n += 1;
        if n >= 40 {
            break;
        }
    }
    LocalFacts {
        ssh_error: None,
        redis_configured,
        engines,
        paths,
        otel_spans,
        otel_detail,
        otel_last_write,
    }
}

fn app_log_paths(apps: &[TracelynApp]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for app in apps {
        for source in &app.log_sources {
            if let Some(path) = source.path.as_deref() {
                if path.starts_with('/') && !out.iter().any(|(_, p)| p == path) {
                    out.push((app.name.clone(), path.to_string()));
                }
            }
        }
    }
    out
}

fn secret_present(server_id: &str, kind: SecretKind) -> bool {
    get_secret(server_id, kind)
        .ok()
        .is_some_and(|value| !value.is_empty())
}

#[tauri::command]
pub fn list_ssh_private_keys() -> SshKeyList {
    let dir = ssh_dir();
    SshKeyList {
        directory: dir.display().to_string(),
        keys: detected_private_keys(),
    }
}

#[tauri::command]
pub fn inspect_ssh_private_key(path: String, passphrase: Option<String>) -> KeyInspection {
    inspect_key_file(Path::new(&path), passphrase.as_deref())
}
