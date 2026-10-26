use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRow {
    pub id: String,
    pub actor: String,
    pub server_id: String,
    pub server_name: String,
    pub target: String,
    pub command: String,
    pub ts_ms: i64,
    pub exit_code: Option<i32>,
    pub output_excerpt: String,
}

#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    pub server_id: String,
    pub target: String,
    pub since_ms: i64,
    pub until_ms: i64,
    pub failed_only: bool,
    pub limit: i64,
}

pub fn migrate(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS audit_log (
            id TEXT PRIMARY KEY,
            actor TEXT NOT NULL,
            server_id TEXT NOT NULL,
            server_name TEXT NOT NULL,
            target TEXT NOT NULL,
            command TEXT NOT NULL,
            ts_ms INTEGER NOT NULL,
            exit_code INTEGER,
            output_excerpt TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_audit_log_ts ON audit_log (ts_ms DESC);
        ",
    )
}

pub fn operations_enabled(conn: &Connection) -> Result<bool, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT value FROM app_settings WHERE key = 'operations_mode'")?;
    let mut rows = stmt.query([])?;
    if let Some(row) = rows.next()? {
        let value: String = row.get(0)?;
        return Ok(value == "1");
    }
    Ok(false)
}

pub fn set_operations_enabled(conn: &Connection, enabled: bool) -> Result<(), rusqlite::Error> {
    let value = if enabled { "1" } else { "0" };
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES ('operations_mode', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![value],
    )?;
    Ok(())
}

pub fn record(conn: &Connection, row: &AuditRow) -> Result<(), rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO audit_log
            (id, actor, server_id, server_name, target, command, ts_ms, exit_code, output_excerpt)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            row.id,
            row.actor,
            row.server_id,
            row.server_name,
            row.target,
            row.command,
            row.ts_ms,
            row.exit_code,
            row.output_excerpt
        ],
    )?;
    tx.execute(
        "DELETE FROM audit_log WHERE id NOT IN (
            SELECT id FROM audit_log ORDER BY ts_ms DESC, id DESC LIMIT 2000
         )",
        [],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn list_audit(conn: &Connection, query: &AuditQuery) -> Result<Vec<AuditRow>, rusqlite::Error> {
    let limit = query.limit.clamp(1, 500);
    let failed = if query.failed_only { 1 } else { 0 };
    let mut stmt = conn.prepare(
        "SELECT id, actor, server_id, server_name, target, command, ts_ms, exit_code, output_excerpt
         FROM audit_log
         WHERE (?1 = '' OR server_id = ?1)
           AND (?2 = '' OR instr(lower(target), lower(?2)) > 0)
           AND (?3 = 0 OR ts_ms >= ?3)
           AND (?4 = 0 OR ts_ms <= ?4)
           AND (?5 = 0 OR IFNULL(exit_code, 1) != 0)
         ORDER BY ts_ms DESC, id DESC
         LIMIT ?6",
    )?;
    let rows = stmt.query_map(
        params![
            query.server_id,
            query.target,
            query.since_ms,
            query.until_ms,
            failed,
            limit
        ],
        |row| {
            Ok(AuditRow {
                id: row.get(0)?,
                actor: row.get(1)?,
                server_id: row.get(2)?,
                server_name: row.get(3)?,
                target: row.get(4)?,
                command: row.get(5)?,
                ts_ms: row.get(6)?,
                exit_code: row.get(7)?,
                output_excerpt: row.get(8)?,
            })
        },
    )?;
    rows.collect()
}

pub fn to_csv(rows: &[AuditRow]) -> String {
    let mut out = String::from("time_ms,actor,server,target,command,exit_code,output\n");
    for row in rows {
        let exit = row
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_default();
        out.push_str(&csv_field(&row.ts_ms.to_string()));
        out.push(',');
        out.push_str(&csv_field(&row.actor));
        out.push(',');
        out.push_str(&csv_field(&row.server_name));
        out.push(',');
        out.push_str(&csv_field(&row.target));
        out.push(',');
        out.push_str(&csv_field(&row.command));
        out.push(',');
        out.push_str(&csv_field(&exit));
        out.push(',');
        out.push_str(&csv_field(&row.output_excerpt));
        out.push('\n');
    }
    out
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, ts: i64, target: &str, exit_code: i32) -> AuditRow {
        AuditRow {
            id: id.into(),
            actor: "ada".into(),
            server_id: "srv".into(),
            server_name: "edge".into(),
            target: target.into(),
            command: "pm2 restart -- 'api'".into(),
            ts_ms: ts,
            exit_code: Some(exit_code),
            output_excerpt: "ok".into(),
        }
    }

    #[test]
    fn mode_defaults_off_and_roundtrips() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        assert!(!operations_enabled(&conn).unwrap());
        set_operations_enabled(&conn, true).unwrap();
        assert!(operations_enabled(&conn).unwrap());
        set_operations_enabled(&conn, false).unwrap();
        assert!(!operations_enabled(&conn).unwrap());
    }

    #[test]
    fn filters_and_csv_and_cap() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        record(&conn, &row("a", 10, "api", 0)).unwrap();
        record(&conn, &row("b", 20, "worker", 1)).unwrap();
        let failed = list_audit(
            &conn,
            &AuditQuery {
                failed_only: true,
                limit: 50,
                ..AuditQuery::default()
            },
        )
        .unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].id, "b");

        let named = list_audit(
            &conn,
            &AuditQuery {
                target: "ap".into(),
                limit: 50,
                ..AuditQuery::default()
            },
        )
        .unwrap();
        assert_eq!(named.len(), 1);
        assert_eq!(named[0].target, "api");

        let mut quoted = row("c", 30, "api", 0);
        quoted.command = "say \"hi\", now".into();
        quoted.output_excerpt = "line\n2".into();
        record(&conn, &quoted).unwrap();
        let csv = to_csv(&list_audit(
            &conn,
            &AuditQuery {
                limit: 10,
                ..AuditQuery::default()
            },
        )
        .unwrap());
        assert!(csv.lines().next().unwrap().starts_with("time_ms,actor,server"));
        assert!(csv.contains("\"say \"\"hi\"\", now\""));
        assert!(csv.contains("\"line\n2\""));

        for n in 0..2005 {
            record(&conn, &row(&format!("n{n}"), 1000 + n, "api", 0)).unwrap();
        }
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))
            .unwrap();
        assert!(count <= 2000);
    }
}
