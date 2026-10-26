use crate::apps::types::TracelynApp;
use rusqlite::{params, Connection};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppStoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn migrate_app_registry(conn: &Connection) -> Result<(), AppStoreError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS app_registry (
            server_id TEXT NOT NULL,
            app_id TEXT NOT NULL,
            json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY (server_id, app_id)
        );
        ",
    )?;
    Ok(())
}

pub fn load_manual_apps(conn: &Connection, server_id: &str) -> Result<Vec<TracelynApp>, AppStoreError> {
    let mut stmt = conn.prepare(
        "SELECT json FROM app_registry WHERE server_id = ?1 ORDER BY updated_at_ms DESC",
    )?;
    let rows = stmt.query_map(params![server_id], |row| {
        let json: String = row.get(0)?;
        Ok(json)
    })?;
    let mut apps = Vec::new();
    for row in rows {
        let json = row?;
        apps.push(serde_json::from_str(&json)?);
    }
    Ok(apps)
}

pub fn upsert_manual_app(conn: &Connection, app: &TracelynApp) -> Result<(), AppStoreError> {
    let json = serde_json::to_string(app)?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO app_registry (server_id, app_id, json, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(server_id, app_id) DO UPDATE SET json = excluded.json, updated_at_ms = excluded.updated_at_ms",
        params![app.server_id, app.id, json, now_ms],
    )?;
    Ok(())
}

pub fn set_app_hidden(
    conn: &Connection,
    server_id: &str,
    app_id: &str,
    hidden: bool,
) -> Result<(), AppStoreError> {
    let mut stmt = conn.prepare("SELECT json FROM app_registry WHERE server_id = ?1 AND app_id = ?2")?;
    let mut rows = stmt.query(params![server_id, app_id])?;
    if let Some(row) = rows.next()? {
        let json: String = row.get(0)?;
        let mut app: TracelynApp = serde_json::from_str(&json)?;
        app.hidden = hidden;
        upsert_manual_app(conn, &app)?;
        return Ok(());
    }
    // Hide discovered app by storing a stub override
    let app = TracelynApp {
        id: app_id.to_string(),
        server_id: server_id.to_string(),
        name: app_id.to_string(),
        kind: crate::apps::types::AppKind::Manual,
        status: crate::apps::types::AppStatus::Unknown,
        origin: crate::apps::types::AppOrigin::Discovered,
        hidden,
        pids: vec![],
        ports: vec![],
        domains: vec![],
        tags: vec![],
        log_sources: vec![],
        cpu_percent: None,
        memory_bytes: None,
        restarts: None,
        uptime_secs: None,
        cwd: None,
        interpreter: None,
        linked_app_ids: vec![],
        metrics: None,
    };
    upsert_manual_app(conn, &app)?;
    Ok(())
}
