pub fn parse_net_bytes(net_dev: &str) -> Option<(u64, u64)> {
    let mut rx = 0u64;
    let mut tx = 0u64;
    for line in net_dev.lines().skip(2) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (iface, rest) = line.split_once(':')?;
        if iface.trim() == "lo" {
            continue;
        }
        let cols: Vec<&str> = rest.split_whitespace().collect();
        if cols.len() < 9 {
            continue;
        }
        rx += cols[0].parse::<u64>().unwrap_or(0);
        tx += cols[8].parse::<u64>().unwrap_or(0);
    }
    Some((rx, tx))
}

pub fn net_rates_bps(
    prev: (u64, u64),
    curr: (u64, u64),
    interval_secs: f64,
) -> (f64, f64) {
    if interval_secs <= 0.0 {
        return (0.0, 0.0);
    }
    let drx = curr.0.saturating_sub(prev.0) as f64;
    let dtx = curr.1.saturating_sub(prev.1) as f64;
    (drx / interval_secs, dtx / interval_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_non_loopback() {
        let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/fixtures/proc_net_dev.txt"))
            .or_else(|_| {
                std::fs::read_to_string("src-tauri/src/collectors/fixtures/proc_net_dev.txt")
            })
            .unwrap();
        let (rx, tx) = parse_net_bytes(&raw).unwrap();
        assert_eq!(rx, 5_000_000_000);
        assert_eq!(tx, 3_000_000_000);
    }

    #[test]
    fn computes_rates() {
        let (in_bps, out_bps) = net_rates_bps((0, 0), (2000, 1000), 2.0);
        assert_eq!(in_bps, 1000.0);
        assert_eq!(out_bps, 500.0);
    }
}
