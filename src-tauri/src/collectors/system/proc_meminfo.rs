fn kb_value(line: &str) -> Option<u64> {
    let mut parts = line.split_whitespace();
    parts.next()?;
    parts.next()?.parse().ok()
}

/// Used memory percentage (0–100).
pub fn memory_used_percent(meminfo: &str) -> Option<f64> {
    let mut total_kb: Option<u64> = None;
    let mut available_kb: Option<u64> = None;
    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = kb_value(line);
        } else if line.starts_with("MemAvailable:") {
            available_kb = kb_value(line);
        }
    }
    let total = total_kb?;
    let available = available_kb?;
    if total == 0 {
        return None;
    }
    let used = total.saturating_sub(available);
    Some((used as f64 / total as f64) * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_meminfo_fixture() {
        let raw = std::fs::read_to_string("src/collectors/fixtures/proc_meminfo.txt")
            .or_else(|_| {
                std::fs::read_to_string("src-tauri/src/collectors/fixtures/proc_meminfo.txt")
            })
            .unwrap();
        let pct = memory_used_percent(&raw).unwrap();
        assert!((pct - 50.0).abs() < 0.1);
    }
}
