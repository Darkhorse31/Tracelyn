use super::model::Resource;

pub fn forecast(row: &mut Resource, samples: &[(i64, f64, f64)]) {
    row.set("sampleCount", samples.len());
    if samples.len() < 6 {
        row.set(
            "confidence",
            "Collect at least 6 hourly samples spanning 24 hours.",
        );
        return;
    }
    let start = samples[0].0;
    let span = (samples.last().unwrap().0 - start) as f64 / 86_400_000.;
    if span < 1. {
        row.set(
            "confidence",
            "Less than 24 hours of history. Forecast unavailable.",
        );
        return;
    }
    let capacity = samples.last().unwrap().2;
    if samples
        .iter()
        .any(|s| (s.2 - capacity).abs() > capacity * 0.01)
    {
        row.set(
            "confidence",
            "Capacity changed. Wait for a stable history window.",
        );
        return;
    }
    let n = samples.len() as f64;
    let mean_x = samples
        .iter()
        .map(|s| (s.0 - start) as f64 / 86_400_000.)
        .sum::<f64>()
        / n;
    let mean_y = samples.iter().map(|s| s.1).sum::<f64>() / n;
    let sxx = samples
        .iter()
        .map(|s| ((s.0 - start) as f64 / 86_400_000. - mean_x).powi(2))
        .sum::<f64>();
    if sxx <= 0. {
        return;
    }
    let slope = samples
        .iter()
        .map(|s| ((s.0 - start) as f64 / 86_400_000. - mean_x) * (s.1 - mean_y))
        .sum::<f64>()
        / sxx;
    let residual = samples
        .iter()
        .map(|s| (s.1 - (mean_y + slope * ((s.0 - start) as f64 / 86_400_000. - mean_x))).powi(2))
        .sum::<f64>();
    let variance = samples.iter().map(|s| (s.1 - mean_y).powi(2)).sum::<f64>();
    let r2 = if variance > 0. {
        (1. - residual / variance).clamp(0., 1.)
    } else {
        0.
    };
    row.set("growthBytesPerDay", slope);
    row.set("rSquared", r2);
    if slope > 0. {
        let days = ((capacity - samples.last().unwrap().1) / slope).max(0.);
        row.set("daysUntilFull", days);
        row.set("confidence",format!("{} confidence · R² {:.2} · {:.1} days observed. Linear trend assumes unchanged workload.",if r2>=0.8 && span>=7. {"Higher"} else {"Low"},r2,span));
        if days < 7. {
            row.state = "critical".into();
        } else if days < 21. {
            row.state = "warning".into();
        }
    } else {
        row.set(
            "confidence",
            "No sustained growth in this window. No exhaustion date predicted.",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regression_and_shrink_and_resize() {
        let mut row = Resource::new("/", "ok");
        let samples: Vec<_> = (0..8)
            .map(|i| (i * 86_400_000, 100. + i as f64 * 10., 300.))
            .collect();
        forecast(&mut row, &samples);
        assert!((row.number("daysUntilFull").unwrap() - 13.).abs() < 0.001);
        assert_eq!(row.state, "warning");
        let mut row = Resource::new("/", "ok");
        forecast(&mut row, &[(0, 0., 100.)]);
        assert!(row.number("daysUntilFull").is_none());
        let mut resized = samples.clone();
        resized[0].2 = 200.;
        forecast(&mut row, &resized);
        assert!(row.number("daysUntilFull").is_none());
        let shrinking: Vec<_> = samples.iter().map(|s| (s.0, 300. - s.1, s.2)).collect();
        forecast(&mut row, &shrinking);
        assert!(row.number("daysUntilFull").is_none());
    }
}
