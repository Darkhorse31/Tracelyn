//! Pure alert decisions: restart loops, quiet hours, cooldown, and dedupe.
//! No I/O.

/// Restarts gained between the earliest and latest sample inside the window.
/// A single sample cannot be a loop. The counter is absolute, not a delta.
pub fn restart_loop_increase(samples: &[(i64, u64)], now_ms: i64, window_ms: i64) -> u64 {
    if window_ms <= 0 || samples.is_empty() {
        return 0;
    }
    let start = now_ms.saturating_sub(window_ms);
    let mut inside: Vec<(i64, u64)> = samples
        .iter()
        .copied()
        .filter(|(ts, _)| *ts >= start && *ts <= now_ms)
        .collect();
    if inside.len() < 2 {
        return 0;
    }
    inside.sort_by_key(|(ts, _)| *ts);
    let first = inside[0].1;
    let last = inside[inside.len() - 1].1;
    last.saturating_sub(first)
}

/// `start` and `end` are minutes from local midnight, in `0..1440`.
/// A window that wraps past midnight is quiet in the evening and the morning.
/// Equal bounds mean quiet hours are off.
pub fn in_quiet_hours(minute_of_day: u16, start: u16, end: u16) -> bool {
    let minute = minute_of_day % 1440;
    let start = start % 1440;
    let end = end % 1440;
    if start == end {
        return false;
    }
    if start < end {
        minute >= start && minute < end
    } else {
        minute >= start || minute < end
    }
}

pub fn minute_of_day(ts_ms: i64, utc_offset_secs: i32) -> u16 {
    let secs = ts_ms.div_euclid(1000).saturating_add(utc_offset_secs as i64);
    (secs.rem_euclid(86_400) / 60) as u16
}

/// True when a new notification is allowed.
pub fn cooldown_elapsed(last_ms: Option<i64>, now_ms: i64, cooldown_ms: i64) -> bool {
    match last_ms {
        None => true,
        Some(last) if cooldown_ms <= 0 => now_ms >= last,
        Some(last) => now_ms.saturating_sub(last) >= cooldown_ms,
    }
}

/// True when this fingerprint has not been delivered inside the window.
pub fn dedupe_admit(last_ms: Option<i64>, now_ms: i64, window_ms: i64) -> bool {
    cooldown_elapsed(last_ms, now_ms, window_ms)
}

/// One crash, restart loop, and deploy share a fingerprint so they do not each page.
pub fn notice_fingerprint(server_id: &str, kind: &str, app_id: &str) -> String {
    let family = match kind {
        "app_crash" | "restart_loop" | "deploy" => "lifecycle",
        "ssh_login" | "ssh_fail_burst" => "auth",
        "redis_down" => "redis",
        other => other,
    };
    format!("{server_id}|{family}|{app_id}")
}

/// First sight of a version is silent. A later different version is a deploy.
pub fn deploy_detected(previous: Option<&str>, next: &str) -> bool {
    match previous {
        Some(prev) if !prev.is_empty() && !next.is_empty() && prev != next => true,
        _ => false,
    }
}

/// The first pass records signatures and stays quiet. Later unseen signatures count.
pub fn new_error_value(known_signatures: bool, unseen: usize) -> f64 {
    if !known_signatures || unseen == 0 {
        0.0
    } else {
        1.0
    }
}

/// Collapse numbers so the same exception with a new id is one type.
pub fn error_signature(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if !(lower.contains("error")
        || lower.contains("exception")
        || lower.contains("fatal")
        || lower.contains("panic"))
    {
        return None;
    }
    let mut out = String::new();
    let mut prev_digit = false;
    for ch in lower.chars() {
        if ch.is_ascii_digit() {
            if !prev_digit {
                out.push('#');
                prev_digit = true;
            }
            continue;
        }
        prev_digit = false;
        if ch.is_ascii_alphanumeric() || ch == ' ' || ch == '_' || ch == '.' || ch == ':' {
            out.push(ch);
        } else {
            out.push(' ');
        }
    }
    let compact = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.len() < 6 {
        None
    } else {
        Some(compact.chars().take(180).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_loop_needs_two_samples_inside_the_window() {
        let samples = [(0, 2u64), (30_000, 4), (90_000, 7), (700_000, 8)];
        assert_eq!(restart_loop_increase(&samples, 90_000, 120_000), 5);
        assert_eq!(restart_loop_increase(&samples, 90_000, 20_000), 0);
        assert_eq!(restart_loop_increase(&[(0, 4)], 0, 60_000), 0);
        assert_eq!(restart_loop_increase(&samples, 700_000, 120_000), 0);
        assert_eq!(restart_loop_increase(&[(0, 9), (10, 3)], 10, 60_000), 0);
    }

    #[test]
    fn quiet_hours_wrap_midnight() {
        assert!(!in_quiet_hours(0, 0, 0));
        assert!(in_quiet_hours(22 * 60, 22 * 60, 7 * 60));
        assert!(in_quiet_hours(6 * 60, 22 * 60, 7 * 60));
        assert!(!in_quiet_hours(12 * 60, 22 * 60, 7 * 60));
        assert!(in_quiet_hours(10, 0, 60));
        assert!(!in_quiet_hours(60, 0, 60));
    }

    #[test]
    fn minute_of_day_uses_the_offset() {
        assert_eq!(minute_of_day(0, 0), 0);
        assert_eq!(minute_of_day(0, 5 * 3600 + 30 * 60), 5 * 60 + 30);
        assert_eq!(minute_of_day(3_600_000, 0), 60);
    }

    #[test]
    fn cooldown_blocks_until_the_window_passes() {
        assert!(cooldown_elapsed(None, 1_000, 60_000));
        assert!(!cooldown_elapsed(Some(1_000), 30_000, 60_000));
        assert!(cooldown_elapsed(Some(1_000), 61_000, 60_000));
        assert!(cooldown_elapsed(Some(1_000), 1_000, 0));
    }

    #[test]
    fn dedupe_collapses_one_crash_into_a_single_fingerprint() {
        let crash = notice_fingerprint("srv", "app_crash", "api");
        let looped = notice_fingerprint("srv", "restart_loop", "api");
        let deploy = notice_fingerprint("srv", "deploy", "api");
        assert_eq!(crash, looped);
        assert_eq!(crash, deploy);
        assert_ne!(crash, notice_fingerprint("srv", "disk_forecast", "api"));
        assert!(dedupe_admit(None, 5_000, 900_000));
        assert!(!dedupe_admit(Some(5_000), 6_000, 900_000));
        assert!(dedupe_admit(Some(5_000), 5_000 + 900_000, 900_000));
    }

    #[test]
    fn deploy_and_new_errors_ignore_the_first_sighting() {
        assert!(!deploy_detected(None, "1.2.0"));
        assert!(!deploy_detected(Some("1.2.0"), "1.2.0"));
        assert!(deploy_detected(Some("1.2.0"), "1.3.0"));
        assert!(!deploy_detected(Some(""), "1.3.0"));
        assert_eq!(new_error_value(false, 4), 0.0);
        assert_eq!(new_error_value(true, 0), 0.0);
        assert_eq!(new_error_value(true, 2), 1.0);
    }

    #[test]
    fn error_signature_ignores_ids() {
        let a = error_signature("Error: connection refused 101").unwrap();
        let b = error_signature("Error: connection refused 404").unwrap();
        assert_eq!(a, b);
        assert!(error_signature("listening on port 80").is_none());
    }
}
