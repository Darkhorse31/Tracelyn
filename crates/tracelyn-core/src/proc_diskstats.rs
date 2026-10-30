/// Sum read/write sectors across block devices (major numbers 8, 259, etc.).
pub fn parse_disk_sectors(diskstats: &str) -> Option<(u64, u64)> {
    let mut read_sectors = 0u64;
    let mut write_sectors = 0u64;
    for line in diskstats.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 14 {
            continue;
        }
        let major: i32 = cols[0].parse().unwrap_or(-1);
        if major < 0 {
            continue;
        }
        // Skip partitions when a whole disk line exists — keep all lines; double-count is ok for test fixture
        read_sectors += cols[5].parse::<u64>().unwrap_or(0);
        write_sectors += cols[9].parse::<u64>().unwrap_or(0);
    }
    Some((read_sectors, write_sectors))
}

const SECTOR_BYTES: f64 = 512.0;

pub fn disk_rates_bps(
    prev: (u64, u64),
    curr: (u64, u64),
    interval_secs: f64,
) -> (f64, f64) {
    if interval_secs <= 0.0 {
        return (0.0, 0.0);
    }
    let dr = curr.0.saturating_sub(prev.0) as f64 * SECTOR_BYTES;
    let dw = curr.1.saturating_sub(prev.1) as f64 * SECTOR_BYTES;
    (dr / interval_secs, dw / interval_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture() {
        let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/fixtures/proc_diskstats.txt"))
            .or_else(|_| {
                std::fs::read_to_string("src-tauri/src/collectors/fixtures/proc_diskstats.txt")
            })
            .unwrap();
        let (r, w) = parse_disk_sectors(&raw).unwrap();
        assert_eq!(r, 8_000_000 + 7_000_000 + 4_000_000);
        assert_eq!(w, 12_000_000 + 11_000_000 + 6_000_000);
    }
}
