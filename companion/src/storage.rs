use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
pub const DAY: i64 = 86_400_000;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    pub ts_ms: i64,
    pub metric_key: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
}
pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let db = Connection::open(path)?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA cache_size=-2048;
        CREATE TABLE IF NOT EXISTS raw(ts INTEGER, metric TEXT, value REAL, PRIMARY KEY(ts,metric));
        CREATE TABLE IF NOT EXISTS minute(ts INTEGER, metric TEXT, sum REAL, count INTEGER, min REAL, max REAL, PRIMARY KEY(ts,metric));
        CREATE TABLE IF NOT EXISTS events(ts INTEGER, rule TEXT, body TEXT);
        CREATE TABLE IF NOT EXISTS runtime(rule TEXT PRIMARY KEY, body TEXT);
        CREATE TABLE IF NOT EXISTS processes(ts INTEGER PRIMARY KEY, body TEXT);
        CREATE TABLE IF NOT EXISTS log_hits(rule TEXT, ts INTEGER, count INTEGER);
        CREATE INDEX IF NOT EXISTS log_hits_rule_ts ON log_hits(rule,ts);")?;
    Ok(db)
}
pub fn insert(
    db: &Connection,
    ts: i64,
    values: &std::collections::HashMap<String, f64>,
) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    for (key, value) in values.iter().filter(|(_, v)| v.is_finite()) {
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO raw VALUES (?1,?2,?3)",
            params![ts, key, value],
        )?;
        if inserted > 0 {
            tx.execute("INSERT INTO minute VALUES (?1,?2,?3,1,?3,?3) ON CONFLICT(ts,metric) DO UPDATE SET sum=sum+excluded.sum,count=count+1,min=MIN(min,excluded.min),max=MAX(max,excluded.max)",params![ts.div_euclid(60_000)*60_000,key,value])?;
        }
    }
    tx.commit()
}
pub fn prune(db: &Connection, now: i64) -> rusqlite::Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute("DELETE FROM raw WHERE ts < ?1", [now - 2 * DAY])?;
    tx.execute("DELETE FROM minute WHERE ts < ?1", [now - 30 * DAY])?;
    tx.execute("DELETE FROM events WHERE ts < ?1", [now - 30 * DAY])?;
    tx.execute("DELETE FROM processes WHERE ts < ?1", [now - 2 * DAY])?;
    tx.execute("DELETE FROM log_hits WHERE ts < ?1", [now - 2 * DAY])?;
    tx.commit()?;
    db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
}
pub fn query(
    db: &Connection,
    since: i64,
    until: i64,
    metric: &str,
    now: i64,
) -> rusqlite::Result<Vec<Point>> {
    // A range crossing retention uses minute buckets throughout. This preserves
    // recent data and keeps a 30 day per-metric request below the response cap.
    let boundary = (now - 2 * DAY).div_euclid(60_000) * 60_000 + 60_000;
    let mut stmt = db.prepare("SELECT ts,metric,sum/count,min,max FROM minute WHERE ts>=?1 AND ts<=?2 AND ?1<?4 AND (?3='' OR metric=?3)
        UNION ALL SELECT ts,metric,value,value,value FROM raw WHERE ts>=?1 AND ts<=?2 AND ?1>=?4 AND (?3='' OR metric=?3) ORDER BY 1 LIMIT 50001")?;
    let rows = stmt.query_map(params![since, until, metric, boundary], |r| {
        Ok(Point {
            ts_ms: r.get(0)?,
            metric_key: r.get(1)?,
            value: r.get(2)?,
            min: r.get(3)?,
            max: r.get(4)?,
        })
    })?;
    let points: Vec<Point> = rows.collect::<rusqlite::Result<_>>()?;
    if points.len() > 50000 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(points)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rollup_average_extrema_and_idempotence() {
        let db = open(":memory:").unwrap();
        for (t, v) in [(0, 2.), (5000, 8.), (5000, 8.)] {
            insert(&db, t, &[("cpu.percent".into(), v)].into()).unwrap();
        }
        let p = query(&db, 0, 60000, "cpu.percent", 3 * DAY).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].value, p[0].min, p[0].max), (5., 2., 8.));
    }
    #[test]
    fn retention_boundaries() {
        let db = open(":memory:").unwrap();
        for t in [0, 28 * DAY, 30 * DAY] {
            insert(&db, t, &[("x".into(), 1.)].into()).unwrap();
        }
        prune(&db, 30 * DAY + 1).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM raw", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM minute", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
}
