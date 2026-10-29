use crate::{state::AppState, storage::RawSample};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tauri::{Manager, State};

fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS workspace_documents(kind TEXT NOT NULL,id TEXT NOT NULL,json TEXT NOT NULL,PRIMARY KEY(kind,id));
    CREATE TABLE IF NOT EXISTS metric_baseline_samples(server TEXT NOT NULL,metric TEXT NOT NULL,hour INTEGER NOT NULL,ts INTEGER NOT NULL,value REAL NOT NULL,PRIMARY KEY(server,metric,ts));
    CREATE INDEX IF NOT EXISTS baseline_lookup ON metric_baseline_samples(server,metric,hour,ts);
    CREATE TABLE IF NOT EXISTS metric_baselines(server TEXT NOT NULL,metric TEXT NOT NULL,hour INTEGER NOT NULL,median REAL NOT NULL,mad REAL NOT NULL,samples INTEGER NOT NULL,updated INTEGER NOT NULL,PRIMARY KEY(server,metric,hour));
    CREATE INDEX IF NOT EXISTS baseline_retention ON metric_baseline_samples(ts);
    CREATE TABLE IF NOT EXISTS synthetic_history(id TEXT NOT NULL,ts INTEGER NOT NULL,json TEXT NOT NULL,PRIMARY KEY(id,ts));
    CREATE TABLE IF NOT EXISTS change_snapshots(server TEXT NOT NULL,ts INTEGER NOT NULL,json TEXT NOT NULL,PRIMARY KEY(server,ts));")
}
fn kind_ok(kind: &str) -> Result<(), String> {
    if matches!(kind, "dashboard" | "synthetic") {
        Ok(())
    } else {
        Err("Unknown document type".into())
    }
}
#[tauri::command]
pub fn documents(state: State<AppState>, kind: String) -> Result<Vec<Value>, String> {
    kind_ok(&kind)?;
    let store = state.metrics.lock();
    let db = store.connection();
    migrate(db).map_err(|e| e.to_string())?;
    let mut stmt = db
        .prepare("SELECT json FROM workspace_documents WHERE kind=?1 ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([kind], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    rows.map(|r| serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string()))
        .collect()
}
#[tauri::command]
pub fn save_document(state: State<AppState>, kind: String, document: Value) -> Result<(), String> {
    crate::license::require_write()?;
    kind_ok(&kind)?;
    let id = document["id"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 128)
        .ok_or("Document needs an ID")?;
    if kind == "synthetic" {
        let interval = document["intervalSeconds"].as_i64().unwrap_or(60);
        if !(15..=86400).contains(&interval) {
            return Err("Interval must be between 15 and 86400 seconds".into());
        }
        validate_workflow(
            &serde_json::from_value(document.clone())
                .map_err(|e| format!("Invalid workflow: {e}"))?,
        )?;
    }
    if kind == "dashboard" {
        validate_dashboard(&document)?;
    }
    let raw = document.to_string();
    if raw.len() > 131072 {
        return Err("Document exceeds 128 KiB".into());
    }
    let store = state.metrics.lock();
    let db = store.connection();
    migrate(db).map_err(|e| e.to_string())?;
    db.execute("INSERT INTO workspace_documents VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET json=excluded.json",params![kind,id,raw]).map_err(|e|e.to_string())?;
    Ok(())
}
#[tauri::command]
pub fn delete_document(state: State<AppState>, kind: String, id: String) -> Result<(), String> {
    crate::license::require_write()?;
    kind_ok(&kind)?;
    let store = state.metrics.lock();
    migrate(store.connection()).map_err(|e| e.to_string())?;
    store
        .connection()
        .execute(
            "DELETE FROM workspace_documents WHERE kind=?1 AND id=?2",
            params![kind, id],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
// UTC hours are stable across daylight saving changes. Keep eight weeks of samples.
pub fn hour_of_week(ts: i64) -> i64 {
    (ts.div_euclid(3_600_000) + 72).rem_euclid(168)
}
pub fn record_baselines(db: &Connection, samples: &[RawSample]) -> rusqlite::Result<()> {
    migrate(db)?;
    let tx = db.unchecked_transaction()?;
    for s in samples.iter().filter(|s| s.value.is_finite()) {
        // One observation per minute keeps the seasonal distribution bounded.
        let changed = tx.execute(
            "INSERT OR IGNORE INTO metric_baseline_samples VALUES(?1,?2,?3,?4,?5)",
            params![
                s.server_id,
                s.metric_key,
                hour_of_week(s.ts_ms),
                s.ts_ms.div_euclid(60000) * 60000,
                s.value
            ],
        )?;
        if changed > 0 {
            let hour = hour_of_week(s.ts_ms);
            let mut stmt=tx.prepare("SELECT value FROM metric_baseline_samples WHERE server=?1 AND metric=?2 AND hour=?3 AND ts>=?4")?;
            let mut values = stmt
                .query_map(
                    params![s.server_id, s.metric_key, hour, s.ts_ms - 56 * 86_400_000],
                    |r| r.get::<_, f64>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if let Some(center) = median(&mut values) {
                let mad = median(
                    &mut values
                        .iter()
                        .map(|v| (v - center).abs())
                        .collect::<Vec<_>>(),
                )
                .unwrap_or(0.);
                tx.execute(
                    "INSERT OR REPLACE INTO metric_baselines VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![
                        s.server_id,
                        s.metric_key,
                        hour,
                        center,
                        mad,
                        values.len(),
                        s.ts_ms
                    ],
                )?;
            }
        }
    }
    if let Some(now) = samples.iter().map(|s| s.ts_ms).max() {
        tx.execute(
            "DELETE FROM metric_baseline_samples WHERE ts<?1",
            [now - 56 * 86_400_000],
        )?;
    }
    tx.commit()
}
pub fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let n = values.len();
    Some(if n % 2 == 0 {
        (values[n / 2 - 1] + values[n / 2]) / 2.
    } else {
        values[n / 2]
    })
}
#[tauri::command]
pub fn baseline(
    state: State<AppState>,
    server_id: String,
    metric: String,
    at: i64,
) -> Result<Value, String> {
    let store = state.metrics.lock();
    let db = store.connection();
    migrate(db).map_err(|e| e.to_string())?;
    let mut stmt=db.prepare("SELECT value FROM metric_baseline_samples WHERE server=?1 AND metric=?2 AND hour=?3 AND ts>=?4 AND ts<?5").map_err(|e|e.to_string())?;
    let mut values = stmt
        .query_map(
            params![
                server_id,
                metric,
                hour_of_week(at),
                at - 56 * 86_400_000,
                at
            ],
            |r| r.get::<_, f64>(0),
        )
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    let center = median(&mut values);
    let mad =
        center.and_then(|m| median(&mut values.iter().map(|v| (v - m).abs()).collect::<Vec<_>>()));
    Ok(json!({"median":center,"mad":mad,"samples":values.len(),"hour":hour_of_week(at)}))
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: String,
    pub server_id: String,
    pub vantage: String,
    pub steps: Vec<Step>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub url: String,
    #[serde(default = "http")]
    pub kind: String,
    #[serde(default = "status")]
    pub status: u16,
    #[serde(default)]
    pub keyword: String,
    #[serde(default)]
    pub extract: BTreeMap<String, String>,
}
fn http() -> String {
    "http".into()
}
fn status() -> u16 {
    200
}
fn validate_workflow(w: &Workflow) -> Result<(), String> {
    if w.id.is_empty() || w.id.len() > 128 || w.server_id.is_empty() {
        return Err("Workflow needs an ID and server".into());
    }
    if !matches!(w.vantage.as_str(), "pc" | "vps") || w.steps.is_empty() || w.steps.len() > 8 {
        return Err("Choose PC or VPS and one to eight steps".into());
    }
    for s in &w.steps {
        if !matches!(s.kind.as_str(), "http" | "ssl" | "dns")
            || s.url.len() > 2048
            || s.keyword.len() > 4096
            || !(100..=599).contains(&s.status)
            || s.extract.len() > 16
        {
            return Err("Invalid check step".into());
        }
    }
    Ok(())
}
fn interpolate(raw: &str, vars: &BTreeMap<String, String>) -> Result<String, String> {
    let mut out = raw.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    if out.contains("{{") {
        return Err("A step references an unavailable variable".into());
    }
    Ok(out)
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn valid_url(raw: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(raw).map_err(|_| "Invalid URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("Use HTTP or HTTPS without credentials or fragments".into());
    }
    Ok(url)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    ok: bool,
    status: u16,
    total_ms: f64,
    dns_ms: f64,
    addresses: String,
    certificate_days: Option<i64>,
    message: String,
}
fn parse_response(
    raw: &str,
    step: &Step,
    vars: &mut BTreeMap<String, String>,
) -> Result<StepResult, String> {
    let (body, meta) = raw
        .rsplit_once("\nTRACELYN_META\n")
        .ok_or("curl did not return timing metadata")?;
    let v: Value =
        serde_json::from_str(meta.trim()).map_err(|_| "curl timing metadata is unavailable")?;
    let status = v["http_code"].as_u64().unwrap_or(0) as u16;
    let mut ok = if step.kind == "dns" {
        v["remote_ip"].as_str().is_some_and(|v| !v.is_empty())
    } else {
        status == step.status && body.contains(&step.keyword)
    };
    let cert = v["certs"]
        .as_str()
        .and_then(|certs| {
            certs
                .lines()
                .find_map(|line| line.strip_prefix("Expire date:"))
        })
        .and_then(|date| {
            chrono::NaiveDateTime::parse_from_str(date.trim(), "%b %e %H:%M:%S %Y GMT").ok()
        })
        .map(|date| {
            (date.and_utc().timestamp_millis() - crate::metrics_ingest::now_ms())
                .div_euclid(86_400_000)
        });
    if step.kind == "ssl" {
        ok &= cert.is_some_and(|days| days >= 0);
    }
    if ok && !step.extract.is_empty() {
        let body: Value =
            serde_json::from_str(body).map_err(|_| "Extraction requires a JSON response")?;
        for (name, pointer) in &step.extract {
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err("Invalid variable name".into());
            }
            let value = body
                .pointer(pointer)
                .ok_or("JSON pointer did not match a response field")?;
            let value = value
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| value.to_string());
            if value.len() > 4096 {
                return Err("Extracted variable exceeds 4 KiB".into());
            }
            vars.insert(name.clone(), value);
        }
    }
    Ok(StepResult {
        ok,
        status,
        total_ms: v["time_total"].as_f64().unwrap_or(0.) * 1000.,
        dns_ms: v["time_namelookup"].as_f64().unwrap_or(0.) * 1000.,
        addresses: v["remote_ip"].as_str().unwrap_or("").into(),
        certificate_days: cert,
        message: if ok {
            "Assertions passed"
        } else {
            "Status, keyword, DNS or certificate assertion failed"
        }
        .into(),
    })
}
async fn execute_workflow(app: tauri::AppHandle, workflow: Workflow) -> Result<Value, String> {
    crate::license::require_write()?;
    validate_workflow(&workflow)?;
    static SLOTS: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();
    let _permit = SLOTS
        .get_or_init(|| tokio::sync::Semaphore::new(4))
        .try_acquire()
        .map_err(|_| "All synthetic workers are busy")?;
    let mut vars = BTreeMap::new();
    let mut results = Vec::new();
    for step in &workflow.steps {
        let url = interpolate(&step.url, &vars)?;
        let parsed = valid_url(&url)?;
        if step.kind == "ssl" && parsed.scheme() != "https" {
            return Err("SSL checks require HTTPS".into());
        }
        if step.kind == "dns" {
            let result = dns_probe(&app, &workflow, &parsed, &step.keyword).await?;
            let ok = result.ok;
            results.push(result);
            if !ok {
                break;
            }
            continue;
        }
        // GET only. No shell interpolation of URLs, response bodies or extracted values.
        let args = vec![
            "--disable",
            "--silent",
            "--show-error",
            "--noproxy",
            "*",
            "--connect-timeout",
            "4",
            "--max-time",
            "10",
            "--max-filesize",
            "1048576",
            "--proto",
            "=http,https",
            "--write-out",
            "\nTRACELYN_META\n%{json}",
            "--url",
            &url,
        ];
        let raw = if workflow.vantage == "pc" {
            let args = args.iter().map(|v| v.to_string()).collect::<Vec<_>>();
            let output = tauri::async_runtime::spawn_blocking(move || {
                let mut cmd =
                    std::process::Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    cmd.creation_flags(0x08000000);
                }
                cmd.args(args).output()
            })
            .await
            .map_err(|_| "PC check worker failed")?
            .map_err(|_| "curl is unavailable")?;
            if !output.status.success() {
                return Err(
                    "PC curl failed. Check DNS, connectivity, response size and certificate trust"
                        .into(),
                );
            }
            String::from_utf8_lossy(&output.stdout).to_string()
        } else {
            let session = app
                .state::<AppState>()
                .session(&workflow.server_id)
                .ok_or("Connect the VPS before running checks")?;
            let command = format!(
                "curl {}",
                args.iter().map(|v| quote(v)).collect::<Vec<_>>().join(" ")
            );
            let slots = tokio::sync::Semaphore::new(1);
            let output = crate::infra::remote::execute(&session, &slots, &command).await?;
            if output.code != 0 {
                return Err(
                    "VPS curl failed. Check DNS, connectivity, response size and certificate trust"
                        .into(),
                );
            }
            output.text
        };
        let result = parse_response(&raw, step, &mut vars)?;
        let ok = result.ok;
        results.push(result);
        if !ok {
            break;
        }
    }
    let ts = crate::metrics_ingest::now_ms();
    let result = json!({"tsMs":ts,"vantage":workflow.vantage,"ok":results.len()==workflow.steps.len() && results.iter().all(|r|r.ok),"steps":results});
    let state = app.state::<AppState>();
    let store = state.metrics.lock();
    let db = store.connection();
    migrate(db).map_err(|e| e.to_string())?;
    db.execute(
        "INSERT OR REPLACE INTO synthetic_history VALUES(?1,?2,?3)",
        params![workflow.id, ts, result.to_string()],
    )
    .map_err(|e| e.to_string())?;
    db.execute("DELETE FROM synthetic_history WHERE id=?1 AND ts NOT IN (SELECT ts FROM synthetic_history WHERE id=?1 ORDER BY ts DESC LIMIT 500)",[&workflow.id]).map_err(|e|e.to_string())?;
    Ok(result)
}
#[tauri::command]
pub fn what_changed(
    state: State<AppState>,
    server_id: String,
    start: i64,
    end: i64,
) -> Result<Value, String> {
    if end <= start || end - start > 86_400_000 {
        return Err("Choose a window up to 24 hours".into());
    }
    let store = state.metrics.lock();
    let db = store.connection();
    migrate(db).map_err(|e| e.to_string())?;
    let means = |a: i64, b: i64| -> Result<BTreeMap<String, f64>, String> {
        let mut stmt=db.prepare("SELECT metric_key,AVG(value) FROM metric_raw WHERE server_id=?1 AND ts_ms>=?2 AND ts_ms<?3 GROUP BY metric_key").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map(params![server_id, a, b], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<BTreeMap<_, _>>>()
            .map_err(|e| e.to_string())
    };
    let before = means(start - 86_400_000, end - 86_400_000)?;
    let current = means(start, end)?;
    let mut movers=current.iter().filter_map(|(k,v)| before.get(k).map(|b|json!({"metric":k,"before":b,"after":v,"delta":v-b,"percent":if b.abs()>1e-9 {Some((v-b)/b.abs()*100.)}else{None}}))).collect::<Vec<_>>();
    movers.sort_by(|a, b| {
        b["percent"]
            .as_f64()
            .unwrap_or(0.)
            .abs()
            .total_cmp(&a["percent"].as_f64().unwrap_or(0.).abs())
    });
    movers.truncate(10);
    let logs = |a: i64, b: i64| -> Result<Vec<String>, String> {
        let mut stmt=db.prepare("SELECT DISTINCT body FROM otel_logs WHERE server_id=?1 AND ts_ms>=?2 AND ts_ms<?3 LIMIT 2000").map_err(|e|e.to_string())?;
        let rows = stmt
            .query_map(params![server_id, a, b], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())
    };
    let pattern = |s: &str| {
        s.split_whitespace()
            .map(|w| {
                if w.chars().any(|c| c.is_ascii_digit()) {
                    "<value>"
                } else {
                    w
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let old = logs(start - 86_400_000, end - 86_400_000)?
        .iter()
        .map(|s| pattern(s))
        .collect::<std::collections::BTreeSet<_>>();
    let patterns = logs(start, end)?
        .iter()
        .map(|s| pattern(s))
        .filter(|s| !old.contains(s))
        .collect::<std::collections::BTreeSet<_>>();
    let mut stmt=db.prepare("SELECT operation,AVG(CASE WHEN start_ms>=?2 THEN duration_us END)/1000.0,AVG(CASE WHEN start_ms<?2 THEN duration_us END)/1000.0 FROM spans WHERE server_id=?1 AND ((start_ms>=?2 AND start_ms<?3) OR (start_ms>=?4 AND start_ms<?5)) GROUP BY operation HAVING AVG(CASE WHEN start_ms>=?2 THEN duration_us END)>AVG(CASE WHEN start_ms<?2 THEN duration_us END) ORDER BY 2 DESC LIMIT 10").map_err(|e|e.to_string())?;
    let endpoints=stmt.query_map(params![server_id,start,end,start-86_400_000,end-86_400_000],|r|Ok(json!({"endpoint":r.get::<_,String>(0)?,"afterMs":r.get::<_,f64>(1)?,"beforeMs":r.get::<_,f64>(2)?}))).map_err(|e|e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e|e.to_string())?;
    Ok(
        json!({"movers":movers,"newLogPatterns":patterns,"slowerEndpoints":endpoints,"infrastructure":infrastructure_diff(db,&server_id,start,end)?,"coverage":"Metrics and ingested OpenTelemetry logs and spans. Infrastructure changes require snapshots from both windows.","start":start,"end":end}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn median_and_mad_resist_outliers() {
        let mut v = vec![1., 2., 2., 3., 100.];
        let m = median(&mut v).unwrap();
        assert_eq!(m, 2.);
        assert_eq!(
            median(&mut v.iter().map(|v| (v - m).abs()).collect::<Vec<_>>()),
            Some(1.)
        );
        assert_eq!(median(&mut []), None);
    }
    #[test]
    fn utc_week_buckets() {
        assert_eq!(hour_of_week(0), 72);
        assert_eq!(hour_of_week(4 * 86_400_000), 0);
        assert_eq!(hour_of_week(7 * 86_400_000), 72);
    }
    #[test]
    fn seasonal_storage_is_bounded_and_deduplicated() {
        let db = Connection::open_in_memory().unwrap();
        let sample = RawSample {
            server_id: "s".into(),
            metric_key: "cpu".into(),
            ts_ms: 60_000,
            value: 2.,
        };
        record_baselines(&db, &[sample.clone(), sample]).unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM metric_baseline_samples", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn variable_resolution_and_shell_quoting() {
        let vars = BTreeMap::from([("id".into(), "42".into())]);
        assert_eq!(
            interpolate("https://a/{{id}}", &vars).unwrap(),
            "https://a/42"
        );
        assert!(interpolate("{{missing}}", &vars).is_err());
        assert_eq!(quote("a'b"), "'a'\\''b'");
        assert!(valid_url("file:///etc/passwd").is_err());
        assert!(valid_url("https://u:p@a/").is_err());
    }
    #[test]
    fn assertions_and_json_pointer_extraction() {
        let s = Step {
            url: "https://a".into(),
            kind: "http".into(),
            status: 201,
            keyword: "token".into(),
            extract: BTreeMap::from([("token".into(), "/token".into())]),
        };
        let mut vars = BTreeMap::new();
        let result = parse_response(
            "{\"token\":\"secret\"}\nTRACELYN_META\n{\"http_code\":201,\"time_total\":0.1}",
            &s,
            &mut vars,
        )
        .unwrap();
        assert!(result.ok);
        assert_eq!(result.total_ms, 100.);
        assert_eq!(vars["token"], "secret");
        assert!(!serde_json::to_string(&result).unwrap().contains("secret"));
    }
}

#[tauri::command]
pub fn synthetic_results(state: State<AppState>, id: String) -> Result<Vec<Value>, String> {
    let store = state.metrics.lock();
    let db = store.connection();
    migrate(db).map_err(|e| e.to_string())?;
    let mut stmt = db
        .prepare("SELECT json FROM synthetic_history WHERE id=?1 ORDER BY ts DESC LIMIT 500")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([id], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut results = rows
        .map(|r| serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    results.reverse();
    Ok(results)
}
pub fn spawn(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut due = std::collections::HashMap::new();
        let mut last_snapshot = 0;
        loop {
            if crate::license::require_write().is_ok() {
                let workflows =
                    documents(app.state::<AppState>(), "synthetic".into()).unwrap_or_default();
                let ids = workflows
                    .iter()
                    .filter_map(|w| w["id"].as_str())
                    .collect::<std::collections::HashSet<_>>();
                due.retain(|id: &String, _| ids.contains(id.as_str()));
                for doc in &workflows {
                    let now = crate::metrics_ingest::now_ms();
                    let Some(id) = doc["id"].as_str() else {
                        continue;
                    };
                    if doc["enabled"] == false || due.get(id).copied().unwrap_or(0) > now {
                        continue;
                    }
                    let interval = doc["intervalSeconds"]
                        .as_i64()
                        .unwrap_or(60)
                        .clamp(15, 86400);
                    due.insert(id.to_string(), now + interval * 1000);
                    if let Ok(workflow) = serde_json::from_value(doc.clone()) {
                        let _ = run_synthetic(app.clone(), workflow).await;
                    }
                }
            }
            let bucket = crate::metrics_ingest::now_ms().div_euclid(300000);
            if bucket != last_snapshot {
                last_snapshot = bucket;
                let _ = capture_infrastructure(&app).await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        }
    });
}
async fn capture_infrastructure(app: &tauri::AppHandle) -> Result<(), String> {
    let servers = app
        .state::<AppState>()
        .config
        .lock()
        .list_servers()
        .to_vec();
    for server in servers {
        let snapshots = crate::infra::get_infra_snapshot(
            app.state::<AppState>(),
            app.state::<crate::infra::InfraState>(),
            server.id.clone(),
            None,
            Some(false),
            Some(false),
        )?;
        let mut selected = snapshots
            .into_iter()
            .filter(|s| {
                matches!(
                    s.collector.as_str(),
                    "ports" | "updates" | "ssh_security" | "fail2ban"
                ) && matches!(
                    s.phase,
                    crate::infra::model::Phase::Ok | crate::infra::model::Phase::Partial
                )
            })
            .collect::<Vec<_>>();
        let session = app.state::<AppState>().session(&server.id);
        if let Some(session) = session {
            let slots = tokio::sync::Semaphore::new(1);
            let command = r#"if command -v dpkg-query >/dev/null 2>&1; then dpkg-query -W -f='${binary:Package}\t${Version}\n' 2>/dev/null | head -n 10000; elif command -v rpm >/dev/null 2>&1; then rpm -qa --qf '%{NAME}\t%{VERSION}-%{RELEASE}\n' 2>/dev/null | head -n 10000; else exit 127; fi"#;
            if let Ok(output) = crate::infra::remote::execute(&session, &slots, command).await {
                if output.code == 0 {
                    let rows = output
                        .text
                        .lines()
                        .filter_map(|line| line.split_once('\t'))
                        .map(|(name, version)| crate::infra::model::Resource::new(name, version))
                        .collect::<Vec<_>>();
                    if !rows.is_empty() {
                        selected.push(crate::infra::model::Snapshot::ok("packages", rows));
                    }
                }
            }
        }
        if selected.is_empty() {
            continue;
        }
        let state = app.state::<AppState>();
        let store = state.metrics.lock();
        let db = store.connection();
        migrate(db).map_err(|e| e.to_string())?;
        let ts = crate::metrics_ingest::now_ms().div_euclid(300000) * 300000;
        db.execute(
            "INSERT OR IGNORE INTO change_snapshots VALUES(?1,?2,?3)",
            params![
                server.id,
                ts,
                serde_json::to_string(&selected).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
        db.execute(
            "DELETE FROM change_snapshots WHERE ts<?1",
            [ts - 56 * 86_400_000],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn infrastructure_diff(
    db: &Connection,
    server: &str,
    start: i64,
    end: i64,
) -> Result<Value, String> {
    let read = |a: i64, b: i64| -> Result<Option<Value>, String> {
        use rusqlite::OptionalExtension;
        let raw=db.query_row("SELECT json FROM change_snapshots WHERE server=?1 AND ts>=?2 AND ts<?3 ORDER BY ts DESC LIMIT 1",params![server,a,b],|r|r.get::<_,String>(0)).optional().map_err(|e|e.to_string())?;
        raw.map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .transpose()
    };
    let before = read(start - 86_400_000, end - 86_400_000)?;
    let after = read(start, end)?;
    let (Some(before), Some(after)) = (before, after) else {
        return Ok(
            json!({"available":false,"message":"Infrastructure snapshots are needed in both windows"}),
        );
    };
    Ok(diff_snapshots(&before, &after))
}
fn diff_snapshots(before: &Value, after: &Value) -> Value {
    let mut changes = Vec::new();
    for snapshot in after.as_array().into_iter().flatten() {
        let collector = &snapshot["collector"];
        let old = before
            .as_array()
            .into_iter()
            .flatten()
            .find(|s| s["collector"] == *collector);
        let Some(old) = old else { continue };
        for row in snapshot["rows"].as_array().into_iter().flatten() {
            let prior = old["rows"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| r["id"] == row["id"]);
            if prior != Some(row) {
                changes.push(json!({"collector":collector,"change":if prior.is_none(){"new"}else{"changed"},"resource":row}));
            }
        }
        for row in old["rows"].as_array().into_iter().flatten() {
            if !snapshot["rows"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|r| r["id"] == row["id"])
            {
                changes.push(json!({"collector":collector,"change":"removed","resource":row}));
            }
        }
    }
    json!({"available":true,"changes":changes})
}

#[cfg(test)]
mod comparison_tests {
    use super::*;
    #[test]
    fn snapshots_report_new_ports_and_changed_security() {
        let old = json!([{"collector":"ports","rows":[{"id":"80","state":"open"}]},{"collector":"ssh_security","rows":[{"id":"ssh","state":"ok"}]}]);
        let new = json!([{"collector":"ports","rows":[{"id":"80","state":"open"},{"id":"443","state":"open"}]},{"collector":"ssh_security","rows":[{"id":"ssh","state":"critical"}]}]);
        let diff = diff_snapshots(&old, &new);
        assert_eq!(diff["changes"].as_array().unwrap().len(), 2);
        assert_eq!(diff["changes"][0]["change"], "new");
        assert_eq!(diff["changes"][1]["change"], "changed");
    }
    #[test]
    fn missing_snapshots_are_explicit() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        assert_eq!(
            infrastructure_diff(&db, "s", 100, 200).unwrap()["available"],
            false
        );
    }
    #[test]
    fn failed_assertions_do_not_extract_secrets() {
        let step = Step {
            url: "https://a".into(),
            kind: "http".into(),
            status: 200,
            keyword: "ok".into(),
            extract: BTreeMap::from([("token".into(), "/token".into())]),
        };
        let mut vars = BTreeMap::new();
        let result = parse_response(
            "{\"token\":\"secret\"}\nTRACELYN_META\n{\"http_code\":500}",
            &step,
            &mut vars,
        )
        .unwrap();
        assert!(!result.ok);
        assert!(vars.is_empty());
    }
}

#[tauri::command]
pub async fn run_synthetic(app: tauri::AppHandle, workflow: Workflow) -> Result<Value, String> {
    crate::license::require_write()?;
    validate_workflow(&workflow)?;
    if !app
        .state::<AppState>()
        .config
        .lock()
        .list_servers()
        .iter()
        .any(|s| s.id == workflow.server_id)
    {
        return Err("Server no longer exists".into());
    }
    match execute_workflow(app.clone(), workflow.clone()).await {
        Ok(result) => Ok(result),
        Err(_) => {
            let ts = crate::metrics_ingest::now_ms();
            let result = json!({"tsMs":ts,"vantage":workflow.vantage,"ok":false,"steps":[{"ok":false,"status":0,"totalMs":0,"dnsMs":0,"addresses":"","certificateDays":null,"message":"Probe failed. Check variables, DNS, connectivity, curl support and certificate trust"}]});
            let state = app.state::<AppState>();
            let store = state.metrics.lock();
            let db = store.connection();
            migrate(db).map_err(|e| e.to_string())?;
            db.execute(
                "INSERT OR REPLACE INTO synthetic_history VALUES(?1,?2,?3)",
                params![workflow.id, ts, result.to_string()],
            )
            .map_err(|e| e.to_string())?;
            db.execute("DELETE FROM synthetic_history WHERE id=?1 AND ts NOT IN (SELECT ts FROM synthetic_history WHERE id=?1 ORDER BY ts DESC LIMIT 500)",[workflow.id]).map_err(|e|e.to_string())?;
            Ok(result)
        }
    }
}

async fn dns_probe(
    app: &tauri::AppHandle,
    workflow: &Workflow,
    url: &url::Url,
    keyword: &str,
) -> Result<StepResult, String> {
    let start = std::time::Instant::now();
    let host = url.host_str().ok_or("DNS check needs a hostname")?;
    let addresses = if workflow.vantage == "pc" {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio::net::lookup_host((host, url.port_or_known_default().unwrap_or(443))),
        )
        .await
        .map_err(|_| "DNS lookup timed out")?
        .map_err(|_| "DNS lookup failed")?;
        result
            .map(|a| a.ip().to_string())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        let session = app
            .state::<AppState>()
            .session(&workflow.server_id)
            .ok_or("Connect the VPS before running checks")?;
        let slots = tokio::sync::Semaphore::new(1);
        let command = format!("getent ahosts {}", quote(host));
        let output = crate::infra::remote::execute(&session, &slots, &command).await?;
        if output.code != 0 {
            return Err("VPS DNS lookup failed".into());
        }
        output
            .text
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ")
    };
    let elapsed = start.elapsed().as_secs_f64() * 1000.;
    let ok = !addresses.is_empty() && addresses.contains(keyword);
    Ok(StepResult {
        ok,
        status: 0,
        total_ms: elapsed,
        dns_ms: elapsed,
        addresses,
        certificate_days: None,
        message: if ok {
            "DNS assertion passed"
        } else {
            "DNS address assertion failed"
        }
        .into(),
    })
}

fn validate_dashboard(document: &Value) -> Result<(), String> {
    if document["version"] != 1
        || document["name"]
            .as_str()
            .is_none_or(|s| s.is_empty() || s.len() > 256)
        || !document["server"].is_string()
    {
        return Err("Invalid dashboard metadata".into());
    }
    let panels = document["panels"]
        .as_array()
        .ok_or("Dashboard needs a panel list")?;
    if panels.len() > 32 {
        return Err("A dashboard supports up to 32 panels".into());
    }
    let mut ids = std::collections::HashSet::new();
    for panel in panels {
        let id = panel["id"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .ok_or("Panel needs an ID")?;
        if !ids.insert(id)
            || !panel["title"].is_string()
            || !panel["metric"].is_string()
            || !matches!(
                panel["kind"].as_str(),
                Some("chart" | "stat" | "logs" | "redis")
            )
        {
            return Err("Invalid panel".into());
        }
        let x = panel["x"].as_u64().ok_or("Invalid panel position")?;
        let y = panel["y"].as_u64().ok_or("Invalid panel position")?;
        let w = panel["w"].as_u64().ok_or("Invalid panel width")?;
        let h = panel["h"].as_u64().ok_or("Invalid panel height")?;
        if x > 11 || y > 1000 || !(1..=12).contains(&w) || x + w > 12 || !(2..=40).contains(&h) {
            return Err("Panel layout is out of bounds".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod dashboard_tests {
    use super::*;
    #[test]
    fn malformed_dashboard_cannot_enter_storage() {
        let mut dashboard = json!({"version":1,"id":"d","name":"Test","server":"$server","panels":[{"id":"p","kind":"chart","title":"CPU","metric":"cpu.percent","x":0,"y":0,"w":6,"h":6}]});
        assert!(validate_dashboard(&dashboard).is_ok());
        dashboard["panels"][0]["w"] = json!(13);
        assert!(validate_dashboard(&dashboard).is_err());
        dashboard["panels"][0]["w"] = json!(6);
        let panel = dashboard["panels"][0].clone();
        dashboard["panels"].as_array_mut().unwrap().push(panel);
        assert!(validate_dashboard(&dashboard).is_err());
    }
    #[test]
    fn package_version_changes_are_detected() {
        let a =
            json!([{"collector":"packages","rows":[{"id":"curl","name":"curl","state":"8.0"}]}]);
        let b =
            json!([{"collector":"packages","rows":[{"id":"curl","name":"curl","state":"8.1"}]}]);
        assert_eq!(diff_snapshots(&a, &b)["changes"][0]["change"], "changed");
    }
}
