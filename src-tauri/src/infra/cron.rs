use super::model::Resource;
use chrono::{TimeZone, Utc};
use std::str::FromStr;

pub fn parse(text: &str, now: i64) -> Vec<Resource> {
    let mut user = "unknown";
    let mut source = "crontab";
    let mut system = false;
    let mut timezone = "UTC".to_string();
    let mut server_timezone = "UTC".to_string();
    let mut rows = vec![];
    let (tables, logs) = text.split_once("__JOURNAL__").unwrap_or((text, ""));
    for line in tables.lines() {
        if let Some(value) = line.strip_prefix("__TZ__\t") {
            server_timezone = value.trim().into();
            timezone = server_timezone.clone();
            continue;
        }
        if let Some(header) = line.strip_prefix("__CRON__\t") {
            let c: Vec<_> = header.split('\t').collect();
            user = c.first().copied().unwrap_or("unknown");
            source = c.get(1).copied().unwrap_or("crontab");
            system = c.get(2) == Some(&"system");
            timezone = server_timezone.clone();
            continue;
        }
        if let Some(value) = line.strip_prefix("CRON_TZ=") {
            timezone = value.trim_matches('"').trim_matches('\'').into();
            continue;
        }
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.contains("crontab:")
            || line.contains("Permission denied")
            || line.contains("not permitted")
            || line.contains("no crontab for")
            || line.split_whitespace().next().unwrap_or("").contains('=')
        {
            continue;
        }
        let parts: Vec<_> = line.split_whitespace().collect();
        let fields = if line.starts_with('@') { 1 } else { 5 };
        let offset = fields + usize::from(system);
        if parts.len() <= offset {
            continue;
        }
        let schedule = parts[..fields].join(" ");
        let owner = if system { parts[fields] } else { user };
        let command = parts[offset..].join(" ");
        let mut row = Resource::new(format!("{owner} · {command}"), "scheduled");
        row.id = format!("{source}:{owner}:{}", rows.len());
        row.set("user", owner);
        row.set("source", source);
        row.set("schedule", schedule.clone());
        row.set("command", command.clone());
        row.set("timezone", timezone.clone());
        match next_run(&schedule, &timezone, now) {
            Ok(Some(next)) => row.set("nextRunMs", next),
            Ok(None) => row.set("nextRun", "At next reboot"),
            Err(err) => {
                row.state = "unknown".into();
                row.set("nextRun", err);
            }
        }
        let last = logs.lines().rev().find(|l| l.contains(&command));
        row.set(
            "lastResult",
            last.map(|l| format!("{l} · execution was logged; exit status unavailable"))
                .unwrap_or_else(|| "No matching journal/syslog record available".into()),
        );
        rows.push(row);
        if rows.len() >= 500 {
            break;
        }
    }
    rows
}
pub fn next_run(expression: &str, timezone: &str, now: i64) -> Result<Option<i64>, String> {
    let normalized = match expression {
        "@reboot" => return Ok(None),
        "@yearly" | "@annually" => "0 0 1 1 *",
        "@monthly" => "0 0 1 * *",
        "@weekly" => "0 0 * * 0",
        "@daily" | "@midnight" => "0 0 * * *",
        "@hourly" => "0 * * * *",
        other => other,
    };
    let fields: Vec<_> = normalized.split_whitespace().collect();
    if fields.len() != 5 {
        return Err("Unsupported cron expression".into());
    }
    let tz: chrono_tz::Tz = timezone
        .parse()
        .map_err(|_| "Server timezone could not be resolved; next run unknown")?;
    let date = Utc
        .timestamp_millis_opt(now)
        .single()
        .ok_or("Invalid current time")?
        .with_timezone(&tz);
    // Vixie cron: Sunday=0 or 7. cron crate: Sunday=1, Saturday=7.
    let dow = translate_dow(fields[4])?;
    let make = |dom: &str, weekday: &str| {
        cron::Schedule::from_str(&format!(
            "0 {} {} {dom} {} {weekday}",
            fields[0], fields[1], fields[3]
        ))
        .map_err(|_| "Unsupported cron expression".to_string())
    };
    let next = if fields[2] != "*" && fields[4] != "*" {
        let a = make(fields[2], "*")?.after(&date).next();
        let b = make("*", &dow)?.after(&date).next();
        [a, b].into_iter().flatten().min()
    } else {
        make(fields[2], &dow)?.after(&date).next()
    };
    next.map(|d| Some(d.timestamp_millis()))
        .ok_or_else(|| "No future run in the supported schedule range".into())
}
fn translate_dow(raw: &str) -> Result<String, String> {
    if raw == "*" {
        return Ok("*".into());
    }
    if raw.chars().any(|c| c.is_ascii_alphabetic()) {
        return Ok(raw.into());
    }
    let mut days = std::collections::BTreeSet::new();
    for token in raw.split(',') {
        let (range, step) = if let Some((a, b)) = token.split_once('/') {
            (a, b.parse::<usize>().map_err(|_| "Invalid weekday step")?)
        } else {
            (token, 1)
        };
        if step == 0 || step > 7 {
            return Err("Invalid weekday step".into());
        }
        let (start, end) = if range == "*" {
            (0, 6)
        } else if let Some((a, b)) = range.split_once('-') {
            (
                a.parse::<usize>().map_err(|_| "Invalid weekday")?,
                b.parse::<usize>().map_err(|_| "Invalid weekday")?,
            )
        } else {
            let n = range.parse::<usize>().map_err(|_| "Invalid weekday")?;
            (n, n)
        };
        if start > end || end > 7 {
            return Err("Invalid weekday range".into());
        }
        for day in (start..=end).step_by(step) {
            days.insert((day % 7 + 1).to_string());
        }
    }
    Ok(days.into_iter().collect::<Vec<_>>().join(","))
}
pub fn timers(text: &str) -> Vec<Resource> {
    text.split("__TIMER__\t")
        .skip(1)
        .take(256)
        .map(|block| {
            let mut lines = block.lines();
            let mut row = Resource::new(lines.next().unwrap_or("timer"), "scheduled");
            row.set("source", "systemd timer");
            for line in lines {
                if let Some((key, value)) = line.split_once('=') {
                    match key {
                        "NextElapseUSecRealtime" => row.set("nextRun", value),
                        "LastTriggerUSec" => row.set("lastRun", value),
                        "Result" => {
                            row.set("lastResult", value);
                            if value != "success" && !value.is_empty() {
                                row.state = "warning".into();
                            }
                        }
                        "ExecMainStatus" => row.set("exitCode", value),
                        "Unit" => row.set("command", value),
                        _ => {}
                    }
                }
            }
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn respects_timezone_and_sunday_and_dom_or_dow() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-26T00:00:00Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(
            next_run("0 6 * * *", "Asia/Kolkata", now).unwrap(),
            Some(now + 1_800_000)
        );
        let sunday = next_run("0 0 * * 0", "UTC", now).unwrap();
        assert_eq!(sunday, next_run("0 0 * * 7", "UTC", now).unwrap());
        assert_eq!(
            next_run("0 0 27 * 0", "UTC", now).unwrap(),
            Some(now + 86_400_000)
        );
        assert!(next_run("bad", "UTC", now).is_err());
        assert!(next_run("0 0 * * *", "Mars/Unknown", now).is_err());
        assert_eq!(next_run("@reboot", "UTC", now).unwrap(), None);
    }
}
