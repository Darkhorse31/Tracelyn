use super::schema::{MetricStore, StoreError};
use rusqlite::params;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryResolution {
    Raw,
    RollupMinute,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionChoice {
    pub resolution: QueryResolution,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricPoint {
    pub ts_ms: i64,
    pub value: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub server_id: String,
    pub metric_key: String,
    pub resolution: QueryResolution,
    pub points: Vec<MetricPoint>,
}

const ONE_HOUR_MS: i64 = 60 * 60 * 1000;
const ONE_DAY_MS: i64 = 24 * ONE_HOUR_MS;

pub fn choose_resolution(
    store: &MetricStore,
    range_start_ms: i64,
    range_end_ms: i64,
    prefer_live: bool,
) -> ResolutionChoice {
    let span = range_end_ms - range_start_ms;
    let raw_cutoff = range_end_ms - store.retention().raw_retention_ms();

    if prefer_live || span <= ONE_HOUR_MS {
        return ResolutionChoice {
            resolution: QueryResolution::Raw,
            reason: if prefer_live {
                "live range prefers raw samples".into()
            } else {
                "range within one hour".into()
            },
        };
    }

    if range_start_ms >= raw_cutoff {
        return ResolutionChoice {
            resolution: QueryResolution::Raw,
            reason: "entire range within raw retention".into(),
        };
    }

    if span > 7 * ONE_DAY_MS {
        return ResolutionChoice {
            resolution: QueryResolution::RollupMinute,
            reason: "range longer than seven days".into(),
        };
    }

    ResolutionChoice {
        resolution: QueryResolution::RollupMinute,
        reason: "range extends beyond raw retention or spans multiple days".into(),
    }
}

pub fn query_metrics(
    store: &MetricStore,
    server_id: &str,
    metric_key: &str,
    range_start_ms: i64,
    range_end_ms: i64,
    prefer_live: bool,
) -> Result<QueryResult, StoreError> {
    let choice = choose_resolution(store, range_start_ms, range_end_ms, prefer_live);
    let points = match choice.resolution {
        QueryResolution::Raw => query_raw(store, server_id, metric_key, range_start_ms, range_end_ms)?,
        QueryResolution::RollupMinute => {
            query_rollup(store, server_id, metric_key, range_start_ms, range_end_ms)?
        }
    };
    Ok(QueryResult {
        server_id: server_id.to_string(),
        metric_key: metric_key.to_string(),
        resolution: choice.resolution,
        points,
    })
}

fn query_raw(
    store: &MetricStore,
    server_id: &str,
    metric_key: &str,
    start: i64,
    end: i64,
) -> Result<Vec<MetricPoint>, StoreError> {
    let mut stmt = store.connection().prepare(
        "SELECT ts_ms, value FROM metric_raw
         WHERE server_id = ?1 AND metric_key = ?2 AND ts_ms >= ?3 AND ts_ms <= ?4
         ORDER BY ts_ms ASC",
    )?;
    let rows = stmt.query_map(params![server_id, metric_key, start, end], |row| {
        Ok(MetricPoint {
            ts_ms: row.get(0)?,
            value: row.get(1)?,
            min: None,
            max: None,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn query_rollup(
    store: &MetricStore,
    server_id: &str,
    metric_key: &str,
    start: i64,
    end: i64,
) -> Result<Vec<MetricPoint>, StoreError> {
    let mut stmt = store.connection().prepare(
        "SELECT bucket_ts_ms, avg_value, min_value, max_value FROM metric_rollup
         WHERE server_id = ?1 AND metric_key = ?2 AND bucket_ts_ms >= ?3 AND bucket_ts_ms <= ?4
         ORDER BY bucket_ts_ms ASC",
    )?;
    let rows = stmt.query_map(params![server_id, metric_key, start, end], |row| {
        Ok(MetricPoint {
            ts_ms: row.get(0)?,
            value: row.get(1)?,
            min: Some(row.get(2)?),
            max: Some(row.get(3)?),
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_prefers_raw_for_short_ranges() {
        let store = MetricStore::open_in_memory().expect("store");
        let end = 1_700_000_000_000i64;
        let start = end - 30 * 60 * 1000;
        let choice = choose_resolution(&store, start, end, false);
        assert_eq!(choice.resolution, QueryResolution::Raw);
    }

    #[test]
    fn resolution_uses_rollup_for_long_history() {
        let store = MetricStore::open_in_memory().expect("store");
        let end = 1_700_000_000_000i64;
        let start = end - 10 * 24 * 60 * 60 * 1000;
        let choice = choose_resolution(&store, start, end, false);
        assert_eq!(choice.resolution, QueryResolution::RollupMinute);
    }
}
