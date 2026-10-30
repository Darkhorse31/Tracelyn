//! Pure rule gates, seasonal EWMA + MAD, incident grouping, and digest timing.
//! No I/O.

use super::types::{is_low_kind, SlotModel};
use crate::types::BandPoint;
use std::collections::HashMap;

pub const JOIN_WINDOW_MS: i64 = 30 * 60 * 1000;
const ALPHA: f64 = 0.2;
const Z_LIMIT: f64 = 3.5;
const WARMUP: u32 = 8;
const RESIDUAL_CAP: usize = 48;

#[derive(Debug, Clone, PartialEq)]
pub struct Gate {
    pub firing: bool,
    pub condition_since_ms: Option<i64>,
    pub clear_since_ms: Option<i64>,
}

impl Default for Gate {
    fn default() -> Self {
        Self {
            firing: false,
            condition_since_ms: None,
            clear_since_ms: None,
        }
    }
}

/// `hot` means the fire condition holds. `cooling` means the value is past the hysteresis band.
pub fn advance_gate(prev: Gate, hot: bool, cooling: bool, ts_ms: i64, duration_ms: i64, clear_ms: i64) -> Gate {
    let mut next = prev;
    if !next.firing {
        if hot {
            let since = next.condition_since_ms.unwrap_or(ts_ms);
            next.condition_since_ms = Some(since);
            if duration_ms <= 0 || ts_ms.saturating_sub(since) >= duration_ms {
                next.firing = true;
                next.clear_since_ms = None;
            }
        } else {
            next.condition_since_ms = None;
        }
        return next;
    }
    if cooling {
        let since = next.clear_since_ms.unwrap_or(ts_ms);
        next.clear_since_ms = Some(since);
        if ts_ms.saturating_sub(since) >= clear_ms {
            next.firing = false;
            next.condition_since_ms = None;
            next.clear_since_ms = None;
        }
    } else {
        next.clear_since_ms = None;
    }
    next
}

pub fn clear_level(kind: &str, threshold: f64, clear: Option<f64>) -> f64 {
    if let Some(value) = clear {
        return value;
    }
    match kind {
        // One day of slack so a cert at exactly the threshold does not flap.
        "ssl_expiry" => threshold + 1.0,
        // Uptime values are 0 or 1. Clear when the check returns up (value > 0).
        "uptime" => threshold,
        _ => threshold * 0.95,
    }
}

pub fn hot_cool(kind: &str, value: f64, threshold: f64, clear: f64) -> (bool, bool) {
    if is_low_kind(kind) {
        (value <= threshold, value > clear)
    } else {
        (value >= threshold, value < clear)
    }
}

pub fn severity_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 2,
        "warning" => 1,
        _ => 0,
    }
}

pub fn max_severity(a: &str, b: &str) -> String {
    if severity_rank(a) >= severity_rank(b) {
        a.to_string()
    } else {
        b.to_string()
    }
}

pub fn hour_of_week(ts_ms: i64) -> u16 {
    let secs = ts_ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let hour = (secs.rem_euclid(86_400) / 3600) as i64;
    // 1970-01-01 was a Thursday. Sunday = 0.
    let weekday = (days + 4).rem_euclid(7);
    (weekday * 24 + hour) as u16
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

fn mad(residuals: &[f64]) -> f64 {
    if residuals.is_empty() {
        return 0.0;
    }
    let mut copy = residuals.to_vec();
    let mid = median(&mut copy);
    let mut devs: Vec<f64> = residuals.iter().map(|v| (v - mid).abs()).collect();
    median(&mut devs)
}

#[derive(Debug, Clone)]
pub struct StepOut {
    pub baseline: f64,
    pub low: f64,
    pub high: f64,
    pub z: f64,
    pub anomalous: bool,
}

/// Score `value` against the seasonal slot, then update it.
///
/// ponytail: alpha 0.2 on every sample, so a slot forgets last week within
/// about a minute of new samples. One update per hour (alpha ~0.3) if
/// week-to-week memory starts to matter.
pub fn step_model(model: &mut SlotModel, value: f64) -> StepOut {
    if !value.is_finite() {
        return StepOut {
            baseline: model.ewma,
            low: model.ewma,
            high: model.ewma,
            z: 0.0,
            anomalous: false,
        };
    }
    if model.samples == 0 {
        model.ewma = value;
        model.samples = 1;
        return StepOut {
            baseline: value,
            low: value,
            high: value,
            z: 0.0,
            anomalous: false,
        };
    }
    let baseline = model.ewma;
    let residual = value - baseline;
    let deviation = mad(&model.residuals);
    let sigma = if deviation == 0.0 {
        0.0
    } else {
        deviation / 0.6745
    };
    let (z, anomalous) = if model.samples < WARMUP {
        (0.0, false)
    } else if sigma == 0.0 {
        if residual.abs() > f64::EPSILON {
            (residual.signum() * 99.0, true)
        } else {
            (0.0, false)
        }
    } else {
        let z = residual / sigma;
        (z, z.abs() >= Z_LIMIT)
    };
    let half = if sigma == 0.0 { 0.0 } else { Z_LIMIT * sigma };
    model.residuals.push(residual);
    if model.residuals.len() > RESIDUAL_CAP {
        let drop = model.residuals.len() - RESIDUAL_CAP;
        model.residuals.drain(0..drop);
    }
    model.ewma = (1.0 - ALPHA) * model.ewma + ALPHA * value;
    model.samples = model.samples.saturating_add(1);
    StepOut {
        baseline,
        low: baseline - half,
        high: baseline + half,
        z,
        anomalous,
    }
}

pub fn replay_bands(points: &[(i64, f64)]) -> Vec<BandPoint> {
    let mut slots: HashMap<u16, SlotModel> = HashMap::new();
    points
        .iter()
        .map(|(ts, value)| {
            let model = slots.entry(hour_of_week(*ts)).or_default();
            let out = step_model(model, *value);
            BandPoint {
                ts_ms: *ts,
                value: *value,
                baseline: out.baseline,
                low: out.low,
                high: out.high,
                z: out.z,
                anomalous: out.anomalous,
            }
        })
        .collect()
}

pub fn log_rate_per_min(count: u64, window_secs: u64) -> f64 {
    let window = window_secs.max(1) as f64;
    count as f64 * 60.0 / window
}

pub fn last_daily_slot(now_ms: i64, hour: u8) -> i64 {
    let hour = (hour as i64).clamp(0, 23);
    let secs = now_ms.div_euclid(1000);
    let day_start = secs.div_euclid(86_400) * 86_400;
    let mut slot = day_start + hour * 3600;
    if slot > secs {
        slot -= 86_400;
    }
    slot * 1000
}

pub fn last_weekly_slot(now_ms: i64, weekday: u8, hour: u8) -> i64 {
    let weekday = (weekday as i64) % 7;
    let daily = last_daily_slot(now_ms, hour) / 1000;
    let days = daily.div_euclid(86_400);
    let wd = (days + 4).rem_euclid(7);
    let back = (wd + 7 - weekday).rem_euclid(7);
    (daily - back * 86_400) * 1000
}

#[derive(Debug, Clone)]
pub struct DueDigest {
    pub cadence: String,
    pub hour_utc: u8,
    pub weekday: u8,
    pub enabled: bool,
    pub last_sent_ms: Option<i64>,
    pub last_attempt_ms: Option<i64>,
}

pub fn digest_due(spec: &DueDigest, now_ms: i64) -> bool {
    if !spec.enabled {
        return false;
    }
    let slot = if spec.cadence == "weekly" {
        last_weekly_slot(now_ms, spec.weekday, spec.hour_utc)
    } else {
        last_daily_slot(now_ms, spec.hour_utc)
    };
    if now_ms < slot {
        return false;
    }
    if spec.last_sent_ms.is_some_and(|sent| sent >= slot) {
        return false;
    }
    if let Some(attempt) = spec.last_attempt_ms {
        if attempt >= slot && now_ms.saturating_sub(attempt) < 3_600_000 {
            return false;
        }
    }
    true
}

/// First observation is silent. A later increase is a restart.
pub fn restart_increase(previous: Option<u64>, next: u64) -> Option<(u64, u64)> {
    match previous {
        Some(prev) if next > prev => Some((prev, next)),
        _ => None,
    }
}

pub fn clip_chars(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let take = max.saturating_sub(1);
    let mut out: String = text.chars().take(take).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_holds_then_fires() {
        let mut gate = Gate::default();
        gate = advance_gate(gate, true, false, 0, 60_000, 30_000);
        assert!(!gate.firing);
        gate = advance_gate(gate, true, false, 59_000, 60_000, 30_000);
        assert!(!gate.firing);
        gate = advance_gate(gate, true, false, 60_000, 60_000, 30_000);
        assert!(gate.firing);
    }

    #[test]
    fn gap_resets_duration() {
        let mut gate = Gate::default();
        gate = advance_gate(gate, true, false, 0, 60_000, 30_000);
        gate = advance_gate(gate, false, true, 10_000, 60_000, 30_000);
        assert!(gate.condition_since_ms.is_none());
        gate = advance_gate(gate, true, false, 20_000, 60_000, 30_000);
        assert!(!gate.firing);
        assert_eq!(gate.condition_since_ms, Some(20_000));
    }

    #[test]
    fn hysteresis_waits_to_clear() {
        let mut gate = Gate {
            firing: true,
            condition_since_ms: Some(0),
            clear_since_ms: None,
        };
        gate = advance_gate(gate, false, true, 1_000, 0, 30_000);
        assert!(gate.firing);
        gate = advance_gate(gate, false, true, 31_000, 0, 30_000);
        assert!(!gate.firing);
    }

    #[test]
    fn low_direction_ssl_and_uptime() {
        let (hot, cool) = hot_cool("ssl_expiry", 10.0, 14.0, clear_level("ssl_expiry", 14.0, None));
        assert!(hot);
        assert!(!cool);
        let (hot, cool) = hot_cool("ssl_expiry", 16.0, 14.0, 15.0);
        assert!(!hot);
        assert!(cool);
        let (hot, cool) = hot_cool("uptime", 0.0, 0.0, clear_level("uptime", 0.0, None));
        assert!(hot);
        assert!(!cool);
        let (hot, cool) = hot_cool("uptime", 1.0, 0.0, 0.0);
        assert!(!hot);
        assert!(cool);
    }

    #[test]
    fn spike_against_flat_season_is_anomalous() {
        let mut model = SlotModel::default();
        for _ in 0..10 {
            let out = step_model(&mut model, 10.0);
            assert!(!out.anomalous);
        }
        let spike = step_model(&mut model, 80.0);
        assert!(spike.anomalous);
        assert!(spike.z > 3.0);
    }

    #[test]
    fn noisy_series_flags_only_the_outlier() {
        let mut model = SlotModel::default();
        let samples = [10.0, 10.4, 9.7, 10.2, 9.9, 10.1, 10.3, 9.8, 10.0, 10.2];
        for value in samples {
            assert!(!step_model(&mut model, value).anomalous);
        }
        assert!(step_model(&mut model, 40.0).anomalous);
    }

    #[test]
    fn slots_do_not_share_a_baseline() {
        let mut points = Vec::new();
        for i in 0..12 {
            points.push((i * 60_000, 10.0));
        }
        let other_hour = 3_600_000;
        for i in 0..12 {
            points.push((other_hour + i * 60_000, 50.0));
        }
        let bands = replay_bands(&points);
        assert!(!bands[11].anomalous, "trained slot stays quiet");
        assert!(!bands[23].anomalous, "second slot trained on its own level");
        let mut back = points.clone();
        back.push((13 * 60_000, 80.0));
        let again = replay_bands(&back);
        assert!(again.last().unwrap().anomalous);
        assert!((again[20].baseline - 50.0).abs() < 1.0);
    }

    #[test]
    fn hour_of_week_epoch_is_thursday() {
        assert_eq!(hour_of_week(0), 4 * 24);
        assert_eq!(hour_of_week(3_600_000), 4 * 24 + 1);
    }

    #[test]
    fn digest_daily_and_retry_gap() {
        let spec = DueDigest {
            cadence: "daily".into(),
            hour_utc: 0,
            weekday: 0,
            enabled: true,
            last_sent_ms: None,
            last_attempt_ms: None,
        };
        assert!(digest_due(&spec, 3_600_000));
        let sent = DueDigest {
            last_sent_ms: Some(0),
            ..spec.clone()
        };
        assert!(!digest_due(&sent, 3_600_000));
        let tried = DueDigest {
            last_attempt_ms: Some(3_600_000),
            ..spec.clone()
        };
        assert!(!digest_due(&tried, 3_600_000 + 60_000));
        assert!(digest_due(&tried, 3_600_000 + 3_600_000));
        let off = DueDigest {
            enabled: false,
            ..spec
        };
        assert!(!digest_due(&off, 3_600_000));
    }

    #[test]
    fn weekly_slot_lands_on_the_requested_weekday() {
        let slot = last_weekly_slot(0, 4, 0);
        assert_eq!(slot, 0);
        let previous = last_weekly_slot(0, 3, 0);
        assert_eq!(previous, -86_400_000);
    }

    #[test]
    fn restart_ignores_first_sight() {
        assert_eq!(restart_increase(None, 4), None);
        assert_eq!(restart_increase(Some(4), 4), None);
        assert_eq!(restart_increase(Some(4), 3), None);
        assert_eq!(restart_increase(Some(4), 6), Some((4, 6)));
    }

    #[test]
    fn rate_scales_to_a_minute() {
        assert!((log_rate_per_min(2, 60) - 2.0).abs() < 1e-9);
        assert!((log_rate_per_min(2, 120) - 1.0).abs() < 1e-9);
    }
}
