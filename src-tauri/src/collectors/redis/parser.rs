use super::types::{RedisDbKeyspace, RedisInfo, RedisSlowlogEntry};

pub fn parse_info_all(raw: &str) -> Result<RedisInfo, String> {
    parse_info_kv(raw)
}

pub fn parse_info_keyspace(raw: &str) -> Result<Vec<RedisDbKeyspace>, String> {
    let mut dbs = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if !line.starts_with("db") {
            continue;
        }
        let (db_part, rest) = line
            .split_once(':')
            .ok_or_else(|| format!("invalid keyspace line: {line}"))?;
        let db = db_part
            .strip_prefix("db")
            .ok_or_else(|| format!("invalid db prefix: {db_part}"))?
            .parse::<u32>()
            .map_err(|e| format!("invalid db number: {e}"))?;

        let mut keys = 0u64;
        let mut expires = 0u64;
        let mut avg_ttl = 0u64;
        for pair in rest.split(',') {
            let (k, v) = pair
                .split_once('=')
                .ok_or_else(|| format!("invalid keyspace pair: {pair}"))?;
            match k.trim() {
                "keys" => keys = v.trim().parse().map_err(|e| format!("keys: {e}"))?,
                "expires" => expires = v.trim().parse().map_err(|e| format!("expires: {e}"))?,
                "avg_ttl" => avg_ttl = v.trim().parse().map_err(|e| format!("avg_ttl: {e}"))?,
                _ => {}
            }
        }
        dbs.push(RedisDbKeyspace {
            db,
            keys,
            expires,
            avg_ttl_ms: avg_ttl,
        });
    }
    dbs.sort_by_key(|d| d.db);
    Ok(dbs)
}

fn parse_info_kv(raw: &str) -> Result<RedisInfo, String> {
    let mut fields = std::collections::BTreeMap::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            fields.insert(k.to_string(), v.to_string());
        }
    }
    Ok(RedisInfo { fields })
}

pub fn parse_config_get_maxmemory(raw: &str) -> Result<u64, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }

    // redis-cli human format: two lines — key then value
    let lines: Vec<&str> = trimmed.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.len() >= 2 && lines[0].contains("maxmemory") {
        return lines[1]
            .parse()
            .map_err(|e| format!("maxmemory value: {e}"));
    }

    // Array-style output
    if let Some(idx) = lines.iter().position(|l| *l == "maxmemory" || l.contains("maxmemory")) {
        if let Some(next) = lines.get(idx + 1) {
            let digits: String = next.chars().filter(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                return digits.parse().map_err(|e| format!("maxmemory digits: {e}"));
            }
        }
    }

    Ok(0)
}

pub fn parse_slowlog_get(raw: &str) -> Result<Vec<RedisSlowlogEntry>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    // Prefer structured array parsing when redis-cli emits RESP-style groups.
    if trimmed.contains("(integer)") || trimmed.starts_with("1)") {
        return parse_slowlog_human(trimmed);
    }

    // Single-line JSON array (redis-cli --json) fallback
    if trimmed.starts_with('[') {
        return parse_slowlog_json(trimmed);
    }

    parse_slowlog_human(trimmed)
}

fn parse_slowlog_human(raw: &str) -> Result<Vec<RedisSlowlogEntry>, String> {
    let mut entries = Vec::new();
    let mut blocks: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        let is_entry_start = trimmed
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
            && trimmed.contains("1) (integer)");
        if is_entry_start && !current.is_empty() {
            blocks.push(current.clone());
            current.clear();
        }
        if !trimmed.is_empty() {
            current.push_str(trimmed);
            current.push('\n');
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }

    for block in blocks {
        let id = block
            .lines()
            .find_map(extract_integer)
            .ok_or_else(|| format!("slowlog block missing id: {block}"))?;
        let integers: Vec<u64> = block.lines().filter_map(extract_integer).collect();
        let ts = integers.get(1).copied().unwrap_or(0);
        let duration = integers.get(2).copied().unwrap_or(0);

        let command_parts: Vec<String> = block
            .lines()
            .filter_map(extract_quoted)
            .collect();
        let command = if command_parts.is_empty() {
            "(unknown)".to_string()
        } else {
            command_parts.join(" ")
        };

        let client = block
            .lines()
            .find(|l| l.trim().starts_with("5)"))
            .map(|l| {
                l.trim()
                    .strip_prefix("5)")
                    .unwrap_or(l)
                    .trim()
                    .trim_matches('"')
                    .to_string()
            })
            .unwrap_or_else(|| "—".to_string());

        entries.push(RedisSlowlogEntry {
            id,
            timestamp_unix: ts,
            duration_micros: duration,
            command,
            client,
        });
    }

    Ok(entries)
}

fn extract_integer(line: &str) -> Option<u64> {
    let start = line.find("(integer)")?;
    let rest = line[start + "(integer)".len()..].trim();
    rest.parse().ok()
}

fn extract_quoted(line: &str) -> Option<String> {
    let start = line.find('"')?;
    let rest = &line[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn parse_slowlog_json(raw: &str) -> Result<Vec<RedisSlowlogEntry>, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("slowlog json: {e}"))?;
    let arr = value
        .as_array()
        .ok_or_else(|| "slowlog json root not array".to_string())?;
    let mut out = Vec::new();
    for item in arr {
        let row = item
            .as_array()
            .ok_or_else(|| "slowlog row not array".to_string())?;
        if row.len() < 4 {
            continue;
        }
        let id = row[0].as_u64().unwrap_or(0);
        let ts = row[1].as_u64().unwrap_or(0);
        let duration = row[2].as_u64().unwrap_or(0);
        let command = row[3]
            .as_array()
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_else(|| row[3].to_string());
        let client = row.get(4).and_then(|c| c.as_str()).unwrap_or("—").to_string();
        out.push(RedisSlowlogEntry {
            id,
            timestamp_unix: ts,
            duration_micros: duration,
            command,
            client,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/redis/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        ))
        .expect("fixture")
    }

    #[test]
    fn parses_info_all_fixture() {
        let info = parse_info_all(&fixture("info_all.txt")).expect("parse");
        assert_eq!(info.get_str("redis_version"), Some("7.2.4"));
        assert_eq!(info.get_u64("connected_clients"), Some(42));
        assert_eq!(info.get_str("role"), Some("master"));
    }

    #[test]
    fn parses_keyspace_fixture() {
        let dbs = parse_info_keyspace(&fixture("info_keyspace.txt")).expect("parse");
        assert_eq!(dbs.len(), 2);
        assert_eq!(dbs[0].keys, 1200);
        assert_eq!(dbs[1].keys, 88);
    }

    #[test]
    fn parses_slowlog_fixture() {
        let entries = parse_slowlog_get(&fixture("slowlog_get.txt")).expect("parse");
        assert_eq!(entries.len(), 2);
        assert!(entries[0].command.contains("GET"));
        assert!(entries[0].duration_micros > 0);
    }

    #[test]
    fn parses_config_maxmemory_fixture() {
        let bytes = parse_config_get_maxmemory(&fixture("config_maxmemory.txt")).expect("parse");
        assert_eq!(bytes, 268_435_456);
    }
}
