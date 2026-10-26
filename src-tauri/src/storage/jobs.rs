use super::schema::{MetricStore, StoreError};
use rusqlite::params;

/// Roll raw samples into 1-minute buckets for data older than `grace_ms` from `now_ms`.
pub fn rollup_raw_to_minute(
    store: &MetricStore,
    now_ms: i64,
    grace_ms: i64,
) -> Result<u64, StoreError> {
    let interval_ms = store.retention().rollup_interval_secs * 1000;
    let cutoff = now_ms - grace_ms;
    let conn = store.connection();

    let tx = conn.unchecked_transaction()?;
    let rolled: u64 = tx.execute(
        "
        INSERT INTO metric_rollup (server_id, metric_key, bucket_ts_ms, min_value, max_value, avg_value, sample_count)
        SELECT
            server_id,
            metric_key,
            (ts_ms / ?1) * ?1 AS bucket_ts_ms,
            MIN(value),
            MAX(value),
            AVG(value),
            COUNT(*)
        FROM metric_raw
        WHERE ts_ms < ?2
        GROUP BY server_id, metric_key, bucket_ts_ms
        ON CONFLICT(server_id, metric_key, bucket_ts_ms) DO UPDATE SET
            min_value = MIN(metric_rollup.min_value, excluded.min_value),
            max_value = MAX(metric_rollup.max_value, excluded.max_value),
            avg_value = (metric_rollup.avg_value * metric_rollup.sample_count + excluded.avg_value * excluded.sample_count)
                / (metric_rollup.sample_count + excluded.sample_count),
            sample_count = metric_rollup.sample_count + excluded.sample_count
        ",
        params![interval_ms, cutoff],
    )? as u64;

    tx.execute("DELETE FROM metric_raw WHERE ts_ms < ?1", params![cutoff])?;
    tx.commit()?;
    Ok(rolled)
}

/// Delete rollups and any remaining raw samples outside retention windows.
pub fn prune_samples(store: &MetricStore, now_ms: i64) -> Result<(u64, u64), StoreError> {
    let raw_cutoff = now_ms - store.retention().raw_retention_ms();
    let rollup_cutoff = now_ms - store.retention().rollup_retention_ms();
    let conn = store.connection();

    let raw_deleted = conn.execute(
        "DELETE FROM metric_raw WHERE ts_ms < ?1",
        params![raw_cutoff],
    )? as u64;
    let rollup_deleted = conn.execute(
        "DELETE FROM metric_rollup WHERE bucket_ts_ms < ?1",
        params![rollup_cutoff],
    )? as u64;
    Ok((raw_deleted, rollup_deleted))
}

pub fn run_maintenance(store: &MetricStore, now_ms: i64) -> Result<(), StoreError> {
    let grace_ms = store.retention().rollup_interval_secs as i64 * 1000 * 2;
    rollup_raw_to_minute(store, now_ms, grace_ms)?;
    prune_samples(store, now_ms)?;
    crate::insight::prune(store.connection(), now_ms, store.retention().raw_retention_ms())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::schema::RawSample;

    fn sample(server: &str, key: &str, ts: i64, value: f64) -> RawSample {
        RawSample {
            server_id: server.into(),
            metric_key: key.into(),
            ts_ms: ts,
            value,
        }
    }

    #[test]
    fn rollup_aggregates_into_minute_buckets() {
        let store = MetricStore::open_in_memory().expect("store");
        let base = 1_700_000_000_000i64;
        store
            .insert_raw_batch(&[
                sample("s1", "cpu.percent", base, 10.0),
                sample("s1", "cpu.percent", base + 15_000, 30.0),
                sample("s1", "cpu.percent", base + 30_000, 50.0),
            ])
            .expect("insert");

        let rolled = rollup_raw_to_minute(&store, base + 120_000, 60_000).expect("rollup");
        assert!(rolled > 0);

        let count: i64 = store
            .connection()
            .query_row("SELECT COUNT(*) FROM metric_rollup", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 1);

        let avg: f64 = store
            .connection()
            .query_row(
                "SELECT avg_value FROM metric_rollup WHERE server_id = 's1'",
                [],
                |r| r.get(0),
            )
            .expect("avg");
        assert!((avg - 30.0).abs() < 0.01);

        let raw_left: i64 = store
            .connection()
            .query_row("SELECT COUNT(*) FROM metric_raw", [], |r| r.get(0))
            .expect("raw");
        assert_eq!(raw_left, 0);
    }

    #[test]
    fn prune_removes_expired_raw_and_rollups() {
        let store = MetricStore::open_in_memory().expect("store");
        let now = 1_800_000_000_000i64;
        let old_raw = now - 8 * 24 * 60 * 60 * 1000;
        let old_rollup = now - 91 * 24 * 60 * 60 * 1000;

        store
            .insert_raw_batch(&[sample("s1", "mem.percent", old_raw, 1.0)])
            .expect("insert raw");
        store
            .connection()
            .execute(
                "INSERT INTO metric_rollup (server_id, metric_key, bucket_ts_ms, min_value, max_value, avg_value, sample_count)
                 VALUES ('s1', 'mem.percent', ?1, 1, 1, 1, 1)",
                params![old_rollup],
            )
            .expect("insert rollup");

        let (raw_d, roll_d) = prune_samples(&store, now).expect("prune");
        assert_eq!(raw_d, 1);
        assert_eq!(roll_d, 1);
    }
}
