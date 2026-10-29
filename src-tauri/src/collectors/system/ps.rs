use crate::apps::types::AppKind;
use crate::apps::TracelynApp;
use crate::metrics::{ProcessRow, ProcessTreeRow};
use std::collections::HashMap;

pub const PS_TREE_CMD: &str =
    "ps -eo pid,ppid,user,pcpu,rss,nlwp,etimes,args --no-headers --sort=-pcpu | head -n 2000";

pub fn parse_ps_top(output: &str, limit: usize) -> Vec<ProcessRow> {
    let mut rows = Vec::new();
    for line in output.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }
        let pid = parts[0].parse().unwrap_or(0);
        let user = parts[1].to_string();
        let cpu = parts[2].parse().unwrap_or(0.0);
        let mem = parts[3].parse().unwrap_or(0.0);
        let command = parts[4..].join(" ");
        rows.push(ProcessRow {
            pid,
            user,
            cpu_percent: cpu,
            memory_percent: mem,
            command,
        });
        if rows.len() >= limit {
            break;
        }
    }
    rows
}

pub fn parse_ps_tree(output: &str, now_ms: u64) -> Vec<ProcessTreeRow> {
    let mut rows = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(pid) = parts.next().and_then(|s| s.parse().ok()) else {
            continue;
        };
        let ppid = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let user = parts.next().unwrap_or("").to_string();
        let cpu_percent = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let rss_kb = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let threads = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let etimes: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let command = parts.collect::<Vec<_>>().join(" ");
        if command.is_empty() {
            continue;
        }
        rows.push(ProcessTreeRow {
            pid,
            ppid,
            user,
            cpu_percent,
            rss_kb,
            threads,
            command,
            started_ms: now_ms.saturating_sub(etimes.saturating_mul(1000)),
            cpu_sparkline: Vec::new(),
            service: None,
            service_kind: None,
        });
    }
    rows
}

/// Match pids onto discovered systemd units, PM2 apps, and Docker containers.
/// Children inherit the nearest mapped ancestor.
pub fn attach_services(rows: &mut [ProcessTreeRow], apps: &[TracelynApp]) {
    let mut by_pid: HashMap<u32, (&str, &str)> = HashMap::new();
    for app in apps {
        let kind = match app.kind {
            AppKind::Systemd => "systemd",
            AppKind::Pm2 => "pm2",
            AppKind::Docker => "docker",
            _ => continue,
        };
        for pid in &app.pids {
            by_pid.insert(*pid, (kind, app.name.as_str()));
        }
    }
    let parents: HashMap<u32, u32> = rows.iter().map(|row| (row.pid, row.ppid)).collect();
    for row in rows.iter_mut() {
        let mut pid = row.pid;
        for _ in 0..8 {
            if let Some((kind, name)) = by_pid.get(&pid) {
                row.service_kind = Some((*kind).to_string());
                row.service = Some((*name).to_string());
                break;
            }
            let Some(next) = parents.get(&pid).copied() else {
                break;
            };
            if next == 0 || next == pid {
                break;
            }
            pid = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture() {
        let raw = std::fs::read_to_string("src/collectors/fixtures/ps.txt")
            .or_else(|_| std::fs::read_to_string("src-tauri/src/collectors/fixtures/ps.txt"))
            .unwrap();
        let rows = parse_ps_top(&raw, 10);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].command, "nginx");
        assert!((rows[0].cpu_percent - 12.5).abs() < 0.01);
    }

    #[test]
    fn parses_tree_columns() {
        let raw = "1 0 root 0.1 2048 1 10 /sbin/init\n42 1 www 12.5 8192 4 3 nginx: master\n";
        let rows = parse_ps_tree(raw, 1_000_000);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].pid, 42);
        assert_eq!(rows[1].ppid, 1);
        assert_eq!(rows[1].rss_kb, 8192);
        assert_eq!(rows[1].threads, 4);
        assert_eq!(rows[1].command, "nginx: master");
        assert_eq!(rows[1].started_ms, 997_000);
    }
}
