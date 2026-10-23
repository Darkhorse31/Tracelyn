pub fn parse_loadavg(loadavg: &str) -> Option<(f64, f64, f64)> {
    let parts: Vec<&str> = loadavg.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    let l1 = parts[0].parse().ok()?;
    let l5 = parts[1].parse().ok()?;
    let l15 = parts[2].parse().ok()?;
    Some((l1, l5, l15))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture() {
        let raw = std::fs::read_to_string("src/collectors/fixtures/proc_loadavg.txt")
            .or_else(|_| {
                std::fs::read_to_string("src-tauri/src/collectors/fixtures/proc_loadavg.txt")
            })
            .unwrap();
        let (a, b, c) = parse_loadavg(&raw).unwrap();
        assert!((a - 1.25).abs() < 0.001);
        assert!((b - 0.98).abs() < 0.001);
        assert!((c - 0.75).abs() < 0.001);
    }
}
