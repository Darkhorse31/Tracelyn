use crate::insight::model::{MapEdge, MapNode, ServiceMap, SpanDetail};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct EdgeSpan {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service: String,
    pub kind: String,
    pub status: String,
    pub duration_us: i64,
    pub peer: Option<String>,
}

pub fn order_spans(spans: Vec<SpanDetail>) -> (Vec<SpanDetail>, Vec<String>) {
    let critical = critical_ids(&spans);
    let crit: HashSet<&str> = critical.iter().map(|id| id.as_str()).collect();
    let ids: HashSet<&str> = spans.iter().map(|s| s.span_id.as_str()).collect();
    let mut children: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (index, span) in spans.iter().enumerate() {
        match span.parent_span_id.as_deref() {
            Some(parent) if ids.contains(parent) => children.entry(parent).or_default().push(index),
            _ => roots.push(index),
        }
    }
    for kids in children.values_mut() {
        kids.sort_by_key(|index| spans[*index].start_ms);
    }
    roots.sort_by_key(|index| spans[*index].start_ms);
    let mut out = Vec::with_capacity(spans.len());
    let mut seen = HashSet::new();
    for root in roots {
        walk(root, 0, &spans, &children, &crit, &mut seen, &mut out);
    }
    for index in 0..spans.len() {
        if !seen.contains(&spans[index].span_id) {
            walk(index, 0, &spans, &children, &crit, &mut seen, &mut out);
        }
    }
    (out, critical)
}

fn walk(
    index: usize,
    depth: u32,
    spans: &[SpanDetail],
    children: &HashMap<&str, Vec<usize>>,
    crit: &HashSet<&str>,
    seen: &mut HashSet<String>,
    out: &mut Vec<SpanDetail>,
) {
    let id = spans[index].span_id.clone();
    if !seen.insert(id.clone()) {
        return;
    }
    let mut row = spans[index].clone();
    row.depth = depth;
    row.critical = crit.contains(row.span_id.as_str());
    out.push(row);
    if let Some(kids) = children.get(id.as_str()) {
        for child in kids {
            walk(*child, depth.saturating_add(1), spans, children, crit, seen, out);
        }
    }
}

pub fn critical_ids(spans: &[SpanDetail]) -> Vec<String> {
    let Some(best) = spans.iter().max_by(|a, b| {
        end_us(a)
            .cmp(&end_us(b))
            .then(a.start_ms.cmp(&b.start_ms))
            .then(a.span_id.cmp(&b.span_id))
    }) else {
        return Vec::new();
    };
    let by_id: HashMap<&str, &SpanDetail> = spans.iter().map(|span| (span.span_id.as_str(), span)).collect();
    let mut chain = Vec::new();
    let mut current = Some(best.span_id.as_str());
    while let Some(id) = current {
        if chain.iter().any(|seen: &String| seen == id) {
            break;
        }
        chain.push(id.to_string());
        current = by_id
            .get(id)
            .and_then(|span| span.parent_span_id.as_deref())
            .filter(|parent| by_id.contains_key(parent));
        if chain.len() > spans.len() {
            break;
        }
    }
    chain.reverse();
    chain
}

fn end_us(span: &SpanDetail) -> i128 {
    span.start_ms as i128 * 1000 + span.duration_us as i128
}

pub fn build_service_map(spans: &[EdgeSpan], window_ms: i64) -> ServiceMap {
    let by_id: HashMap<&str, &EdgeSpan> = spans.iter().map(|span| (span.span_id.as_str(), span)).collect();
    let mut nodes: HashMap<String, (u64, u64)> = HashMap::new();
    let mut edges: HashMap<(String, String), Vec<f64>> = HashMap::new();
    let mut edge_errors: HashMap<(String, String), u64> = HashMap::new();
    for span in spans {
        let entry = nodes.entry(span.service.clone()).or_insert((0, 0));
        entry.0 += 1;
        if span.status == "error" {
            entry.1 += 1;
        }
        if let Some((source, target)) = edge_of(span, &by_id) {
            nodes.entry(target.clone()).or_insert((0, 0));
            let key = (source, target);
            edges.entry(key.clone()).or_default().push(span.duration_us as f64 / 1000.0);
            if span.status == "error" {
                *edge_errors.entry(key).or_insert(0) += 1;
            }
        }
    }
    let secs = (window_ms.max(1) as f64) / 1000.0;
    let mut edge_rows = Vec::new();
    for (key, mut samples) in edges {
        let requests = samples.len() as u64;
        let errors = edge_errors.get(&key).copied().unwrap_or(0);
        edge_rows.push(MapEdge {
            source: key.0,
            target: key.1,
            requests,
            errors,
            rate_per_sec: requests as f64 / secs,
            error_rate: if requests == 0 { 0.0 } else { errors as f64 / requests as f64 },
            p95_ms: percentile_95(&mut samples),
        });
    }
    edge_rows.sort_by(|a, b| b.requests.cmp(&a.requests).then(a.source.cmp(&b.source)));
    let mut node_rows: Vec<MapNode> = nodes
        .into_iter()
        .map(|(label, (span_count, error_count))| MapNode {
            id: label.clone(),
            label,
            span_count,
            error_count,
        })
        .collect();
    node_rows.sort_by(|a, b| a.label.cmp(&b.label));
    ServiceMap {
        nodes: node_rows,
        edges: edge_rows,
        window_ms,
        truncated: false,
    }
}

fn edge_of(span: &EdgeSpan, by_id: &HashMap<&str, &EdgeSpan>) -> Option<(String, String)> {
    match span.kind.as_str() {
        "client" | "producer" => {
            let peer = span.peer.as_ref().filter(|peer| !peer.is_empty())?;
            if peer == &span.service {
                return None;
            }
            Some((span.service.clone(), peer.clone()))
        }
        "server" | "consumer" => {
            let parent_id = span.parent_span_id.as_deref()?;
            let parent = by_id.get(parent_id)?;
            if parent.service == span.service {
                return None;
            }
            let client_already = matches!(parent.kind.as_str(), "client" | "producer") && parent.peer.is_some();
            if client_already {
                return None;
            }
            Some((parent.service.clone(), span.service.clone()))
        }
        _ => None,
    }
}

pub fn percentile_95(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = (0.95 * (values.len() as f64 - 1.0)).round() as usize;
    values[rank.min(values.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn span(id: &str, parent: Option<&str>, start: i64, dur_us: i64) -> SpanDetail {
        SpanDetail {
            span_id: id.into(),
            parent_span_id: parent.map(|s| s.into()),
            service: "api".into(),
            operation: id.into(),
            kind: "internal".into(),
            status: "ok".into(),
            start_ms: start,
            duration_us: dur_us,
            app_id: None,
            peer: None,
            attributes: json!({}),
            events: json!([]),
            depth: 0,
            critical: false,
        }
    }

    #[test]
    fn critical_path_follows_the_latest_child() {
        let spans = vec![
            span("root", None, 0, 100_000),
            span("a", Some("root"), 0, 40_000),
            span("b", Some("root"), 50, 50_000),
        ];
        let (ordered, critical) = order_spans(spans);
        assert_eq!(critical, vec!["root".to_string(), "b".to_string()]);
        assert_eq!(ordered[0].span_id, "root");
        assert_eq!(ordered[0].depth, 0);
        assert!(ordered[0].critical);
        assert_eq!(ordered[1].span_id, "a");
        assert_eq!(ordered[1].depth, 1);
        assert!(!ordered[1].critical);
        assert!(ordered.iter().any(|s| s.span_id == "b" && s.critical));
    }

    #[test]
    fn service_map_uses_client_peer_and_server_parent() {
        let spans = vec![
            EdgeSpan {
                span_id: "c1".into(),
                parent_span_id: None,
                service: "api".into(),
                kind: "client".into(),
                status: "ok".into(),
                duration_us: 10_000,
                peer: Some("auth".into()),
            },
            EdgeSpan {
                span_id: "s1".into(),
                parent_span_id: Some("c1".into()),
                service: "auth".into(),
                kind: "server".into(),
                status: "error".into(),
                duration_us: 8_000,
                peer: None,
            },
            EdgeSpan {
                span_id: "in".into(),
                parent_span_id: Some("gw".into()),
                service: "api".into(),
                kind: "server".into(),
                status: "ok".into(),
                duration_us: 20_000,
                peer: None,
            },
            EdgeSpan {
                span_id: "gw".into(),
                parent_span_id: None,
                service: "gateway".into(),
                kind: "internal".into(),
                status: "ok".into(),
                duration_us: 30_000,
                peer: None,
            },
        ];
        let map = build_service_map(&spans, 60_000);
        let client = map.edges.iter().find(|e| e.source == "api" && e.target == "auth").unwrap();
        assert_eq!(client.requests, 1);
        assert_eq!(client.errors, 0);
        assert!((client.p95_ms - 10.0).abs() < 0.01);
        let inbound = map.edges.iter().find(|e| e.source == "gateway" && e.target == "api").unwrap();
        assert_eq!(inbound.requests, 1);
        assert!(map.nodes.iter().any(|n| n.label == "auth"));
    }
}
