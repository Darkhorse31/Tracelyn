use crate::insight::model::{OtelSuggestion, SlowEngineStatus};

pub fn validate_log_path(path: &str) -> Result<(), String> {
    if path.is_empty() || path.len() > 512 {
        return Err("Path is empty or too long".into());
    }
    if !path.starts_with('/') {
        return Err("Path must be absolute".into());
    }
    if path.split('/').any(|part| part == "..") {
        return Err("Path must not contain ..".into());
    }
    if path
        .chars()
        .any(|c| matches!(c, '\0' | '\n' | '\r' | '\'' | '"' | ';' | '`' | '$'))
    {
        return Err("Path contains characters that are not allowed".into());
    }
    if path.ends_with(".gz") || path.ends_with(".gzip") {
        return Err("Gzip-rotated files are ignored".into());
    }
    Ok(())
}

pub fn mysql_snippet() -> &'static str {
    "[mysqld]\nslow_query_log = 1\nslow_query_log_file = /var/log/mysql/slow.log\nlong_query_time = 0.5\nlog_output = FILE\n"
}

pub fn postgres_snippet() -> &'static str {
    "log_min_duration_statement = 100\nlogging_collector = on\nlog_directory = 'log'\nlog_filename = 'postgresql.log'\n"
}

pub fn mongo_snippet() -> &'static str {
    "systemLog:\n  destination: file\n  path: /var/log/mongodb/mongod.log\noperationProfiling:\n  slowOpThresholdMs: 100\n  mode: slowOp\n"
}

pub fn mysql_hint(enabled: bool, has_path: bool) -> String {
    if !enabled {
        "MySQL slow query logging is off, so Tracelyn cannot tell which statements are slow. Set slow_query_log and long_query_time (the sample uses 0.5 seconds), then restart MySQL or run SET GLOBAL. Tracelyn only reads the log file.".into()
    } else if !has_path {
        "slow_query_log is on, but slow_query_log_file was not found in a config the SSH user can read. Set slow_query_log_file to an absolute path.".into()
    } else {
        "Reading the MySQL slow query log.".into()
    }
}

pub fn postgres_hint(enabled: bool, has_path: bool) -> String {
    if !enabled {
        "PostgreSQL is not recording slow statements. log_min_duration_statement is unset or -1. Set it to a millisecond threshold such as 100 and turn logging_collector on so the lines land in a file Tracelyn can read.".into()
    } else if !has_path {
        "log_min_duration_statement is set, but log_directory or data_directory was not found. Set both so the log file path is known.".into()
    } else {
        "Reading the PostgreSQL log for duration lines.".into()
    }
}

pub fn mongo_hint(enabled: bool, has_path: bool) -> String {
    if !has_path {
        "MongoDB is not writing a file Tracelyn can read. Set systemLog.destination to file and systemLog.path. Slow operations are included when operationProfiling.slowOpThresholdMs is set (100 milliseconds is the usual default).".into()
    } else if !enabled {
        "A MongoDB log path was found, but systemLog.destination is not file. Switch it to file so slow operation JSON lines are on disk.".into()
    } else {
        "Reading the MongoDB log for slow operation lines.".into()
    }
}

pub struct Detection {
    pub suggestions: Vec<OtelSuggestion>,
    pub engines: Vec<SlowEngineStatus>,
}

pub fn detection_script(app_dirs: &[(String, String)]) -> String {
    let mut script = String::from(
        "echo OTEL\nfind /var/log/otel /var/log/opentelemetry -maxdepth 2 -type f \\( -name '*.json' -o -name '*.jsonl' -o -name '*.log' \\) ! -name '*.gz' 2>/dev/null | head -n 40\n",
    );
    for (id, cwd) in app_dirs.iter().take(20) {
        if !safe_token(id) || validate_log_path(cwd).is_err() {
            continue;
        }
        script.push_str(&format!("echo APP {id}\n"));
        script.push_str(&format!(
            "find '{cwd}' -maxdepth 2 -type f \\( -name 'otel*.json' -o -name 'otel*.jsonl' -o -name '*traces*.jsonl' \\) ! -name '*.gz' 2>/dev/null | head -n 10\necho ENDAPP\n"
        ));
    }
    script.push_str(
        "echo MYSQL\nfor f in /etc/my.cnf /etc/mysql/my.cnf /etc/mysql/mysql.conf.d/mysqld.cnf /etc/mysql/mariadb.conf.d/50-server.cnf; do [ -r \"$f\" ] && echo \"FILE $f\" && grep -E '^[[:space:]]*(slow_query_log|slow_query_log_file|long_query_time|log_output)' \"$f\"; done\necho PG\nfor f in /etc/postgresql/*/main/postgresql.conf /var/lib/pgsql/data/postgresql.conf /var/lib/pgsql/*/data/postgresql.conf; do [ -r \"$f\" ] && echo \"FILE $f\" && grep -E '^[[:space:]]*(log_min_duration_statement|log_directory|log_filename|data_directory|logging_collector)' \"$f\"; done\necho MONGO\nfor f in /etc/mongod.conf /etc/mongodb.conf; do [ -r \"$f\" ] && echo \"FILE $f\" && grep -E 'slowOp|systemLog|destination:|path:' \"$f\"; done\nexit 0\n",
    );
    script
}

pub fn parse_detect_output(text: &str) -> Detection {
    let mut section = "pre";
    let mut app_id = String::new();
    let mut suggestions = Vec::new();
    let mut mysql = String::new();
    let mut pg = String::new();
    let mut mongo = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        match trimmed {
            "OTEL" => {
                section = "otel";
                app_id.clear();
                continue;
            }
            "ENDAPP" => {
                section = "otel";
                app_id.clear();
                continue;
            }
            "MYSQL" => {
                section = "mysql";
                continue;
            }
            "PG" => {
                section = "pg";
                continue;
            }
            "MONGO" => {
                section = "mongo";
                continue;
            }
            _ => {}
        }
        if let Some(id) = trimmed.strip_prefix("APP ") {
            if safe_token(id) {
                section = "app";
                app_id = id.to_string();
            }
            continue;
        }
        match section {
            "otel" | "app" => {
                if let Some(path) = clean_path(trimmed) {
                    let reason = if section == "app" {
                        "Found next to the application directory".into()
                    } else {
                        "Found in a common OpenTelemetry log directory".into()
                    };
                    suggestions.push(OtelSuggestion {
                        path,
                        app_id: if section == "app" { app_id.clone() } else { String::new() },
                        reason,
                    });
                }
            }
            "mysql" => {
                mysql.push_str(line);
                mysql.push('\n');
            }
            "pg" => {
                pg.push_str(line);
                pg.push('\n');
            }
            "mongo" => {
                mongo.push_str(line);
                mongo.push('\n');
            }
            _ => {}
        }
    }
    suggestions.sort_by(|a, b| a.path.cmp(&b.path));
    suggestions.dedup_by(|a, b| a.path == b.path && a.app_id == b.app_id);
    Detection {
        suggestions,
        engines: vec![
            mysql_status(&mysql),
            postgres_status(&pg),
            mongo_status(&mongo),
        ],
    }
}

fn mysql_status(text: &str) -> SlowEngineStatus {
    let mut enabled = false;
    let mut saw = false;
    let mut path = None;
    for line in text.lines() {
        let Some((key, value)) = split_setting(line) else {
            continue;
        };
        if key == "slow_query_log_file" {
            path = clean_path(&value);
        } else if key == "slow_query_log" {
            saw = true;
            enabled = is_on(&value);
        }
    }
    let has_path = path.is_some();
    SlowEngineStatus {
        engine: "mysql".into(),
        path,
        source: "auto".into(),
        logging_on: saw && enabled,
        is_dir: false,
        hint: mysql_hint(saw && enabled, has_path),
        snippet: mysql_snippet().into(),
    }
}

fn postgres_status(text: &str) -> SlowEngineStatus {
    let mut threshold: Option<i64> = None;
    let mut directory = None;
    let mut filename = None;
    let mut data_directory = None;
    for line in text.lines() {
        let Some((key, value)) = split_setting(line) else {
            continue;
        };
        match key.as_str() {
            "log_min_duration_statement" => threshold = value.parse().ok(),
            "log_directory" => directory = Some(value),
            "log_filename" => filename = Some(value),
            "data_directory" => data_directory = clean_path(&value),
            _ => {}
        }
    }
    let enabled = threshold.is_some_and(|n| n >= 0);
    let (path, is_dir) = postgres_path(directory.as_deref(), filename.as_deref(), data_directory.as_deref());
    SlowEngineStatus {
        engine: "postgres".into(),
        path: path.clone(),
        source: "auto".into(),
        logging_on: enabled,
        is_dir,
        hint: postgres_hint(enabled, path.is_some()),
        snippet: postgres_snippet().into(),
    }
}

fn postgres_path(directory: Option<&str>, filename: Option<&str>, data_directory: Option<&str>) -> (Option<String>, bool) {
    let Some(directory) = directory else {
        return (None, false);
    };
    let dir = if directory.starts_with('/') {
        directory.to_string()
    } else if let Some(data) = data_directory {
        format!("{}/{}", data.trim_end_matches('/'), directory)
    } else {
        return (None, false);
    };
    let Some(filename) = filename else {
        return (clean_path(&dir), true);
    };
    if filename.contains('%') {
        return (clean_path(&dir), true);
    }
    (clean_path(&format!("{}/{}", dir.trim_end_matches('/'), filename)), false)
}

fn mongo_status(text: &str) -> SlowEngineStatus {
    let mut destination = None;
    let mut path = None;
    for line in text.lines() {
        let Some((key, value)) = split_setting(line) else {
            continue;
        };
        if key == "destination" {
            destination = Some(value);
        } else if key == "path" || key.ends_with(".path") {
            path = clean_path(&value);
        }
    }
    let file_dest = destination
        .as_deref()
        .is_none_or(|value| value.eq_ignore_ascii_case("file"));
    let has_path = path.is_some();
    SlowEngineStatus {
        engine: "mongo".into(),
        path,
        source: "auto".into(),
        logging_on: has_path && file_dest,
        is_dir: false,
        hint: mongo_hint(file_dest, has_path),
        snippet: mongo_snippet().into(),
    }
}

fn split_setting(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with("FILE ") || line.starts_with('#') || line.starts_with(';') {
        return None;
    }
    let (key, value) = if let Some(pair) = line.split_once('=') {
        pair
    } else {
        line.split_once(':')?
    };
    let key = key.trim().trim_start_matches('-').to_ascii_lowercase();
    let value = value
        .split('#')
        .next()
        .unwrap_or(value)
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .to_string();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some((key, value))
}

fn is_on(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "1" | "on" | "true" | "yes")
}

fn clean_path(value: &str) -> Option<String> {
    let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
    validate_log_path(value).ok()?;
    Some(value.to_string())
}

fn safe_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() < 80
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_otel_paths_and_database_configs() {
        let text = "\
OTEL
/var/log/otel/traces.json
/var/log/otel/old.json.gz
APP app_1
/srv/api/otel.jsonl
ENDAPP
MYSQL
FILE /etc/mysql/my.cnf
slow_query_log = 0
slow_query_log_file = /var/log/mysql/slow.log
PG
FILE /etc/postgresql/16/main/postgresql.conf
log_min_duration_statement = -1
log_directory = 'log'
log_filename = 'postgresql-%Y-%m-%d.log'
data_directory = '/var/lib/postgresql/16/main'
MONGO
FILE /etc/mongod.conf
  destination: file
  path: /var/log/mongodb/mongod.log
  slowOpThresholdMs: 100
";
        let found = parse_detect_output(text);
        assert_eq!(found.suggestions.len(), 2);
        assert_eq!(found.suggestions[0].path, "/srv/api/otel.jsonl");
        assert_eq!(found.suggestions[0].app_id, "app_1");
        assert!(found.suggestions.iter().any(|s| s.path == "/var/log/otel/traces.json"));

        let mysql = &found.engines[0];
        assert!(!mysql.logging_on);
        assert_eq!(mysql.path.as_deref(), Some("/var/log/mysql/slow.log"));
        assert!(mysql.hint.contains("slow_query_log"));
        assert!(mysql.snippet.contains("long_query_time"));

        let pg = &found.engines[1];
        assert!(!pg.logging_on);
        assert!(pg.is_dir);
        assert_eq!(pg.path.as_deref(), Some("/var/lib/postgresql/16/main/log"));
        assert!(pg.hint.contains("log_min_duration_statement"));

        let mongo = &found.engines[2];
        assert!(mongo.logging_on);
        assert_eq!(mongo.path.as_deref(), Some("/var/log/mongodb/mongod.log"));
        assert!(mongo.snippet.contains("slowOpThresholdMs"));
    }

    #[test]
    fn postgres_enabled_file_and_mongo_syslog() {
        let text = "\
PG
log_min_duration_statement = 100
log_directory = '/var/log/postgresql'
log_filename = 'postgresql.log'
MONGO
destination: syslog
path: /var/log/mongodb/mongod.log
";
        let found = parse_detect_output(text);
        let pg = found.engines.iter().find(|e| e.engine == "postgres").unwrap();
        assert!(pg.logging_on);
        assert!(!pg.is_dir);
        assert_eq!(pg.path.as_deref(), Some("/var/log/postgresql/postgresql.log"));
        let mongo = found.engines.iter().find(|e| e.engine == "mongo").unwrap();
        assert!(!mongo.logging_on);
        assert!(mongo.hint.contains("systemLog.destination"));
    }

    #[test]
    fn script_skips_unsafe_paths() {
        let script = detection_script(&[
            ("app_1".into(), "/srv/api".into()),
            ("bad id".into(), "/tmp".into()),
            ("ok".into(), "/tmp/a'b".into()),
        ]);
        assert!(script.contains("echo APP app_1"));
        assert!(script.contains("find '/srv/api'"));
        assert!(!script.contains("bad id"));
        assert!(!script.contains("a'b"));
        assert!(script.contains("slow_query_log"));
    }
}
