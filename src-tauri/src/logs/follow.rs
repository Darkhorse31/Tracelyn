use crate::apps::types::{LogSourceKind, LogSourceRef, TracelynApp};
use crate::events::SourceStatus;
use crate::logs::LogRingBuffer;
use crate::ssh::{start_command_follow, start_command_follow_stdin, SshSession};
use std::collections::HashMap;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::Receiver;

const MAX_TAILS: usize = 8;
const DRAIN_PER_SOURCE: usize = 400;

struct TailSlot {
    key: String,
    app_id: String,
    source_id: String,
    rx: Receiver<Result<String, String>>,
}

pub struct TailHub {
    slots: Vec<TailSlot>,
    generation: u64,
    statuses: HashMap<String, SourceStatus>,
}

impl TailHub {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            generation: 0,
            statuses: HashMap::new(),
        }
    }

    pub fn statuses(&self) -> &HashMap<String, SourceStatus> {
        &self.statuses
    }

    pub fn mark_stale(&mut self) {
        self.generation = self.generation.wrapping_sub(1);
    }

    pub async fn sync(
        &mut self,
        session: &SshSession,
        apps: &[TracelynApp],
        app_ids: Option<&[String]>,
        source_kinds: Option<&[String]>,
        generation: u64,
        sudo_password: Option<&str>,
    ) {
        if generation == self.generation && !self.slots.is_empty() {
            return;
        }
        self.generation = generation;
        let desired = desired_sources(apps, app_ids, source_kinds);
        let desired_keys: Vec<String> = desired.iter().map(|(k, _, _)| k.clone()).collect();
        self.slots.retain(|slot| desired_keys.iter().any(|k| k == &slot.key));

        for (key, app_id, source) in desired {
            if self.slots.iter().any(|s| s.key == key) {
                continue;
            }
            let inner = follow_command(&source);
            let (cmd, stdin) = if sudo_password.is_some() {
                (
                    crate::setup::sudo_stdin_read(&inner),
                    sudo_password.map(crate::setup::password_bytes),
                )
            } else {
                (crate::setup::privileged_read(&inner), None)
            };
            let started = if stdin.is_some() {
                start_command_follow_stdin(session, &cmd, stdin).await
            } else {
                start_command_follow(session, &cmd).await
            };
            match started {
                Ok(rx) => {
                    self.statuses.insert(
                        source.id.clone(),
                        SourceStatus {
                            ok: true,
                            reason: None,
                            fix_hint: None,
                        },
                    );
                    self.slots.push(TailSlot {
                        key,
                        app_id,
                        source_id: source.id,
                        rx,
                    });
                }
                Err(err) => {
                    self.statuses.insert(
                        source.id.clone(),
                        status_from_message(&err.to_string()),
                    );
                }
            }
        }
    }

    pub fn drain(&mut self, buf: &mut LogRingBuffer) {
        for slot in &mut self.slots {
            for _ in 0..DRAIN_PER_SOURCE {
                match slot.rx.try_recv() {
                    Ok(Ok(line)) => {
                        buf.push(&slot.source_id, Some(&slot.app_id), &line);
                        self.statuses.insert(
                            slot.source_id.clone(),
                            SourceStatus {
                                ok: true,
                                reason: None,
                                fix_hint: None,
                            },
                        );
                    }
                    Ok(Err(msg)) => {
                        self.statuses
                            .insert(slot.source_id.clone(), status_from_message(&msg));
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.statuses.insert(
                            slot.source_id.clone(),
                            SourceStatus {
                                ok: false,
                                reason: Some("log follow stopped".into()),
                                fix_hint: Some(
                                    "Rediscover applications. The SSH channel for this log closed."
                                        .into(),
                                ),
                            },
                        );
                        break;
                    }
                }
            }
        }
    }
}

fn desired_sources(
    apps: &[TracelynApp],
    app_ids: Option<&[String]>,
    source_kinds: Option<&[String]>,
) -> Vec<(String, String, LogSourceRef)> {
    let mut out = Vec::new();
    for app in apps {
        if app.hidden {
            continue;
        }
        if let Some(ids) = app_ids {
            if !ids.is_empty() && !ids.iter().any(|id| id == &app.id) {
                continue;
            }
        }
        for source in &app.log_sources {
            if matches!(
                source.kind,
                LogSourceKind::NginxAccess | LogSourceKind::ApacheAccess
            ) {
                // ponytail: access logs already followed by WebPipeline
                continue;
            }
            let kind_name = kind_name(source.kind);
            if let Some(kinds) = source_kinds {
                if !kinds.is_empty()
                    && !kinds
                        .iter()
                        .any(|k| k.eq_ignore_ascii_case(kind_name) || k == &source.id)
                {
                    continue;
                }
            }
            let key = format!(
                "{}:{}:{}",
                app.id,
                source.id,
                source.path.clone().unwrap_or_else(|| source.container_id.clone().unwrap_or_default())
            );
            out.push((key, app.id.clone(), source.clone()));
        }
    }
    out.sort_by_key(|(_, _, src)| source_rank(src.kind));
    out.truncate(MAX_TAILS);
    out
}

fn kind_name(kind: LogSourceKind) -> &'static str {
    match kind {
        LogSourceKind::Stdout => "stdout",
        LogSourceKind::Stderr => "stderr",
        LogSourceKind::Journald => "journald",
        LogSourceKind::Docker => "docker",
        LogSourceKind::NginxAccess => "nginx_access",
        LogSourceKind::NginxError => "nginx_error",
        LogSourceKind::ApacheAccess => "apache_access",
        LogSourceKind::Custom => "custom",
    }
}

fn source_rank(kind: LogSourceKind) -> u8 {
    match kind {
        LogSourceKind::Stderr => 0,
        LogSourceKind::Stdout => 1,
        LogSourceKind::NginxError => 2,
        LogSourceKind::Docker => 3,
        LogSourceKind::Journald => 4,
        LogSourceKind::Custom => 5,
        LogSourceKind::NginxAccess | LogSourceKind::ApacheAccess => 9,
    }
}

fn follow_command(source: &LogSourceRef) -> String {
    match source.kind {
        LogSourceKind::Journald => {
            let unit = source.path.clone().unwrap_or_default().replace('\'', "");
            format!(
                "sudo -n journalctl -u '{unit}' -n 500 -f -o cat --no-pager 2>&1 || journalctl -u '{unit}' -n 500 -f -o cat --no-pager 2>&1"
            )
        }
        LogSourceKind::Docker => {
            let id = shell_quote(source.container_id.as_deref().unwrap_or(""));
            format!("docker logs -f --tail 500 --since 15m {id} 2>&1")
        }
        _ => {
            let path = source.path.clone().unwrap_or_default();
            if path.is_empty() {
                "echo 'No such file: log path is empty'".into()
            } else if path.contains('*') || path.contains('?') {
                let pattern = shell_quote(&path);
                let script = format!(
                    "mapfile -t files < <(compgen -G {pattern}); if [ ${{#files[@]}} -eq 0 ]; then echo 'No such file: no matching logs'; exit 1; fi; exec tail -n 500 -F -- \"${{files[@]}}\""
                );
                format!("bash -c {}", shell_quote(&script))
            } else {
                let escaped = path.replace('\'', "'\\''");
                format!("tail -n 500 -F -- '{escaped}' 2>&1")
            }
        }
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub fn status_from_message(msg: &str) -> SourceStatus {
    let lower = msg.to_lowercase();
    if lower.contains("permission denied") {
        SourceStatus {
            ok: false,
            reason: Some("permission denied".into()),
            fix_hint: Some(
                "Use Set up monitoring to allow access."
                    .into(),
            ),
        }
    } else if lower.contains("no such file") || lower.contains("cannot open") {
        SourceStatus {
            ok: false,
            reason: Some("missing".into()),
            fix_hint: Some(
                "The log file is missing or was rotated away. Confirm the path, or wait for the process to recreate it."
                    .into(),
            ),
        }
    } else {
        SourceStatus {
            ok: false,
            reason: Some(msg.chars().take(180).collect()),
            fix_hint: Some("Check the path and that the SSH user can read it.".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::types::{AppKind, AppOrigin, AppStatus};

    fn app_with(kind: LogSourceKind, path: &str) -> TracelynApp {
        TracelynApp {
            id: "app-1".into(),
            server_id: "s".into(),
            name: "api".into(),
            kind: AppKind::Pm2,
            status: AppStatus::Online,
            origin: AppOrigin::Discovered,
            hidden: false,
            pids: vec![],
            ports: vec![],
            domains: vec![],
            tags: vec![],
            log_sources: vec![LogSourceRef {
                id: "src".into(),
                kind,
                path: Some(path.into()),
                container_id: None,
                vhost: None,
                readable: true,
                issue: None,
                fix_hint: None,
            }],
            cpu_percent: None,
            memory_bytes: None,
            restarts: None,
            uptime_secs: None,
            cwd: None,
            interpreter: None,
            linked_app_ids: vec![],
            metrics: None,
        }
    }

    #[test]
    fn skips_access_logs_and_caps() {
        let apps = vec![
            app_with(LogSourceKind::NginxAccess, "/var/log/nginx/a.log"),
            app_with(LogSourceKind::Stderr, "/var/log/app.err"),
        ];
        let desired = desired_sources(&apps, None, None);
        assert_eq!(desired.len(), 1);
        assert!(desired[0].2.path.as_deref().unwrap().contains("app.err"));
    }

    #[test]
    fn permission_hint() {
        let st = status_from_message("tail: cannot open '/var/log/x': Permission denied");
        assert!(!st.ok);
        assert_eq!(st.reason.as_deref(), Some("permission denied"));
        assert!(st.fix_hint.unwrap().contains("Set up monitoring"));
    }
}
