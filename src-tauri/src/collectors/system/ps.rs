use crate::metrics::ProcessRow;

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
}
