use crate::metrics::SsSummary;

pub fn parse_ss_summary(output: &str) -> SsSummary {
    let mut tcp_total = None;
    let mut tcp_established = None;
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("TCP:") {
            if let Some(rest) = trimmed.strip_prefix("TCP:") {
                if let Some(num) = rest.split_whitespace().next() {
                    tcp_total = num.trim_matches(|c| c == '(').parse().ok();
                }
                if let Some(estab_idx) = rest.find("estab") {
                    let after = &rest[estab_idx + 5..];
                    let num: String = after
                        .chars()
                        .skip_while(|c| !c.is_ascii_digit())
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if !num.is_empty() {
                        tcp_established = num.parse().ok();
                    }
                }
            }
        }
    }
    SsSummary {
        tcp_total,
        tcp_established,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture() {
        let raw = std::fs::read_to_string("src/collectors/fixtures/ss_s.txt")
            .or_else(|_| std::fs::read_to_string("src-tauri/src/collectors/fixtures/ss_s.txt"))
            .unwrap();
        let s = parse_ss_summary(&raw);
        assert_eq!(s.tcp_total, Some(150));
        assert_eq!(s.tcp_established, Some(80));
    }
}
