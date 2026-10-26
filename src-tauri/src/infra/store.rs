use super::model::*;
use rusqlite::{params, Connection};

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS infra_settings(server_id TEXT NOT NULL, kind TEXT NOT NULL, json TEXT NOT NULL, PRIMARY KEY(server_id,kind));
        CREATE TABLE IF NOT EXISTS infra_domains(server_id TEXT NOT NULL, domain TEXT NOT NULL, PRIMARY KEY(server_id,domain));
        CREATE TABLE IF NOT EXISTS infra_checks(id TEXT PRIMARY KEY, server_id TEXT NOT NULL, json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS infra_check_results(check_id TEXT NOT NULL, ts_ms INTEGER NOT NULL, json TEXT NOT NULL, PRIMARY KEY(check_id,ts_ms));
        CREATE INDEX IF NOT EXISTS infra_result_time ON infra_check_results(ts_ms);
        CREATE TABLE IF NOT EXISTS infra_disk(server_id TEXT NOT NULL, mount TEXT NOT NULL, ts_ms INTEGER NOT NULL, used REAL NOT NULL, total REAL NOT NULL, PRIMARY KEY(server_id,mount,ts_ms));")
}
pub fn target(db: &Connection, server: &str, engine: &str) -> Target {
    db.query_row(
        "SELECT json FROM infra_settings WHERE server_id=?1 AND kind=?2",
        params![server, engine],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|s| serde_json::from_str(&s).ok())
    .unwrap_or_default()
}
pub fn save_target(
    db: &Connection,
    server: &str,
    engine: &str,
    value: &Target,
) -> Result<(), String> {
    db.execute("INSERT INTO infra_settings VALUES(?1,?2,?3) ON CONFLICT(server_id,kind) DO UPDATE SET json=excluded.json",params![server,engine,serde_json::to_string(value).map_err(|e| e.to_string())?]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn domains(db: &Connection, server: &str) -> rusqlite::Result<Vec<String>> {
    let mut stmt = db.prepare(
        "SELECT domain FROM infra_domains WHERE server_id=?1 ORDER BY domain LIMIT 2048",
    )?;
    let rows = stmt.query_map([server], |r| r.get(0))?.collect();
    rows
}
pub fn save_domains(db: &Connection, server: &str, names: &[String]) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute("DELETE FROM infra_domains WHERE server_id=?1", [server])?;
    for name in names.iter().take(2048) {
        tx.execute(
            "INSERT OR IGNORE INTO infra_domains VALUES(?1,?2)",
            params![server, name],
        )?;
    }
    tx.commit()
}
pub fn checks(db: &Connection, server: Option<&str>) -> rusqlite::Result<Vec<UptimeCheck>> {
    let mut stmt = db.prepare(
        "SELECT json FROM infra_checks WHERE (?1 IS NULL OR server_id=?1) ORDER BY id LIMIT 128",
    )?;
    let raw = stmt
        .query_map([server], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(raw
        .iter()
        .filter_map(|s| serde_json::from_str(s).ok())
        .collect())
}
pub fn check_views(db: &Connection, server: &str) -> rusqlite::Result<Vec<CheckView>> {
    checks(db,Some(server))?.into_iter().map(|check| {
        let mut stmt=db.prepare("SELECT json FROM infra_check_results WHERE check_id=?1 ORDER BY ts_ms DESC LIMIT 240")?;
        let raw=stmt.query_map([&check.id], |r| r.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let history=raw.iter().rev().filter_map(|s|serde_json::from_str(s).ok()).collect();
        Ok(CheckView{check,history})
    }).collect()
}
pub fn save_result(db: &Connection, id: &str, result: &CheckResult) -> rusqlite::Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO infra_check_results VALUES(?1,?2,?3)",
        params![
            id,
            result.ts_ms,
            serde_json::to_string(result).unwrap_or_default()
        ],
    )?;
    db.execute("DELETE FROM infra_check_results WHERE check_id=?1 AND ts_ms NOT IN (SELECT ts_ms FROM infra_check_results WHERE check_id=?1 ORDER BY ts_ms DESC LIMIT 10000)",[id])?;
    db.execute(
        "DELETE FROM infra_check_results WHERE ts_ms<?1",
        [crate::metrics_ingest::now_ms() - 90 * 86_400_000],
    )?;
    Ok(())
}
pub fn save_disk(
    db: &Connection,
    server: &str,
    rows: &[Resource],
    now: i64,
) -> rusqlite::Result<()> {
    let now = now / 300_000 * 300_000;
    let tx = db.unchecked_transaction()?;
    for row in rows {
        if let (Some(used), Some(total)) = (row.number("usedBytes"), row.number("totalBytes")) {
            tx.execute(
                "INSERT OR REPLACE INTO infra_disk VALUES(?1,?2,?3,?4,?5)",
                params![server, row.name, now, used, total],
            )?;
        }
    }
    tx.execute(
        "DELETE FROM infra_disk WHERE ts_ms<?1",
        [now - 30 * 86_400_000],
    )?;
    tx.commit()
}
pub fn disk_history(
    db: &Connection,
    server: &str,
    mount: &str,
) -> rusqlite::Result<Vec<(i64, f64, f64)>> {
    let mut stmt=db.prepare("SELECT min(ts_ms),avg(used),max(total) FROM infra_disk WHERE server_id=?1 AND mount=?2 AND ts_ms>?3 GROUP BY ts_ms/3600000 ORDER BY min(ts_ms) LIMIT 744")?;
    let rows = stmt
        .query_map(
            params![
                server,
                mount,
                crate::metrics_ingest::now_ms() - 30 * 86_400_000
            ],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?
        .collect();
    rows
}
