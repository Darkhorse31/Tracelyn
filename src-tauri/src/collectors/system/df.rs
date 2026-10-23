/// Root mount (`/`) used percentage from `df -P` output.
pub fn parse_root_disk_percent(df_output: &str) -> Option<f64> {
    for line in df_output.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 6 {
            continue;
        }
        let mount = cols[5];
        if mount == "/" {
            let cap = cols[4].trim_end_matches('%');
            return cap.parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_root() {
        let raw = std::fs::read_to_string("src/collectors/fixtures/df_p.txt")
            .or_else(|_| std::fs::read_to_string("src-tauri/src/collectors/fixtures/df_p.txt"))
            .unwrap();
        let pct = parse_root_disk_percent(&raw).unwrap();
        assert!((pct - 48.0).abs() < 0.01);
    }
}
