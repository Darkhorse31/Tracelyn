/// Returns (total_jiffies, idle_jiffies) from the aggregate `cpu` line.
pub fn parse_cpu_jiffies(stat: &str) -> Option<(u64, u64)> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let parts: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|s| s.parse().ok())
        .collect();
    if parts.len() < 4 {
        return None;
    }
    let total: u64 = parts.iter().sum();
    let idle = parts[3] + parts.get(4).copied().unwrap_or(0);
    Some((total, idle))
}

pub fn cpu_usage_percent(prev: (u64, u64), curr: (u64, u64)) -> f64 {
    let dt = curr.0.saturating_sub(prev.0);
    let di = curr.1.saturating_sub(prev.1);
    if dt == 0 {
        return 0.0;
    }
    let used = dt.saturating_sub(di);
    (used as f64 / dt as f64) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("src/collectors/fixtures/{name}"))
            .or_else(|_| {
                std::fs::read_to_string(format!("src-tauri/src/collectors/fixtures/{name}"))
            })
            .expect("fixture")
    }

    #[test]
    fn parses_cpu_line() {
        let a = parse_cpu_jiffies(&fixture("proc_stat.txt")).unwrap();
        let b = parse_cpu_jiffies(&fixture("proc_stat_idle.txt")).unwrap();
        assert!(a.0 > 0 && a.1 > 0);
        let pct = cpu_usage_percent(a, b);
        assert!(pct >= 0.0 && pct <= 100.0);
        assert!(pct > 0.0);
    }
}
