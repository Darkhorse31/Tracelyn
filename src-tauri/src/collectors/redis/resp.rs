//! RESP2 codec. Reads are incremental so a split TCP chunk still parses.

#[derive(Debug, Clone, PartialEq)]
pub enum Resp {
    Simple(String),
    Error(String),
    Int(i64),
    Bulk(Option<Vec<u8>>),
    Array(Vec<Resp>),
}

pub struct RespParser {
    buf: Vec<u8>,
}

impl RespParser {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn next(&mut self) -> Result<Option<Resp>, String> {
        match parse_at(&self.buf, 0)? {
            Some((value, used)) => {
                self.buf.drain(..used);
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
}

impl Default for RespParser {
    fn default() -> Self {
        Self::new()
    }
}

pub fn encode_command(args: &[&str]) -> Vec<u8> {
    let mut out = format!("*{}\r\n", args.len()).into_bytes();
    for arg in args {
        let bytes = arg.as_bytes();
        out.extend(format!("${}\r\n", bytes.len()).into_bytes());
        out.extend_from_slice(bytes);
        out.extend_from_slice(b"\r\n");
    }
    out
}

/// Commands sent on a fresh connection. No HELLO. AUTH only when a password is set.
pub fn handshake_commands(password: Option<&str>, username: Option<&str>) -> Vec<Vec<String>> {
    let mut cmds = Vec::new();
    if let Some(password) = password.filter(|p| !p.is_empty()) {
        let mut auth = Vec::new();
        auth.push("AUTH".to_string());
        if let Some(username) = username.filter(|u| !u.is_empty()) {
            auth.push(username.to_string());
        }
        auth.push(password.to_string());
        cmds.push(auth);
    }
    cmds.push(vec!["INFO".into(), "all".into()]);
    cmds.push(vec!["SLOWLOG".into(), "GET".into(), "128".into()]);
    cmds.push(vec!["INFO".into(), "keyspace".into()]);
    cmds.push(vec!["CONFIG".into(), "GET".into(), "maxmemory".into()]);
    cmds
}

pub fn bulk_text(value: &Resp) -> Result<String, String> {
    match value {
        Resp::Bulk(Some(bytes)) => Ok(String::from_utf8_lossy(bytes).into_owned()),
        Resp::Simple(text) => Ok(text.clone()),
        Resp::Error(text) => Err(text.clone()),
        Resp::Int(n) => Ok(n.to_string()),
        Resp::Bulk(None) | Resp::Array(_) => Err("expected a Redis bulk string".into()),
    }
}

pub fn maxmemory_from_resp(value: &Resp) -> Result<u64, String> {
    match value {
        Resp::Array(items) => {
            let text = items
                .get(1)
                .map(bulk_text)
                .transpose()?
                .unwrap_or_default();
            if text.is_empty() {
                Ok(0)
            } else {
                text.trim().parse().map_err(|e| format!("maxmemory: {e}"))
            }
        }
        other => {
            let text = bulk_text(other)?;
            if text.trim().is_empty() {
                Ok(0)
            } else {
                text.trim().parse().map_err(|e| format!("maxmemory: {e}"))
            }
        }
    }
}

pub fn slowlog_from_resp(value: &Resp) -> Result<Vec<super::types::RedisSlowlogEntry>, String> {
    let Resp::Array(rows) = value else {
        return Err("slowlog was not an array".into());
    };
    let mut out = Vec::new();
    for row in rows {
        let Resp::Array(fields) = row else {
            continue;
        };
        let id = int_at(fields, 0);
        let timestamp_unix = int_at(fields, 1);
        let duration_micros = int_at(fields, 2);
        let command = match fields.get(3) {
            Some(Resp::Array(parts)) => {
                let joined = parts
                    .iter()
                    .map(display)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                if joined.is_empty() {
                    "(unknown)".into()
                } else {
                    joined
                }
            }
            Some(other) => {
                let text = display(other);
                if text.is_empty() {
                    "(unknown)".into()
                } else {
                    text
                }
            }
            None => "(unknown)".into(),
        };
        let client = fields
            .get(4)
            .map(display)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "—".into());
        out.push(super::types::RedisSlowlogEntry {
            id,
            timestamp_unix,
            duration_micros,
            command,
            client,
        });
    }
    Ok(out)
}

fn int_at(fields: &[Resp], index: usize) -> u64 {
    match fields.get(index) {
        Some(Resp::Int(n)) if *n >= 0 => *n as u64,
        Some(other) => display(other).parse().unwrap_or(0),
        None => 0,
    }
}

fn display(value: &Resp) -> String {
    match value {
        Resp::Simple(s) | Resp::Error(s) => s.clone(),
        Resp::Int(n) => n.to_string(),
        Resp::Bulk(Some(b)) => String::from_utf8_lossy(b).into_owned(),
        Resp::Bulk(None) => String::new(),
        Resp::Array(_) => String::new(),
    }
}

fn parse_at(buf: &[u8], start: usize) -> Result<Option<(Resp, usize)>, String> {
    if start >= buf.len() {
        return Ok(None);
    }
    let Some(line_end) = find_crlf(buf, start) else {
        return Ok(None);
    };
    let line = &buf[start..line_end];
    let after = line_end + 2;
    if line.is_empty() {
        return Err("empty RESP line".into());
    }
    let head = std::str::from_utf8(&line[1..]).map_err(|_| "RESP line is not utf-8".to_string())?;
    match line[0] {
        b'+' => Ok(Some((Resp::Simple(head.to_string()), after))),
        b'-' => Ok(Some((Resp::Error(head.to_string()), after))),
        b':' => {
            let n = head.parse::<i64>().map_err(|_| format!("bad integer {head}"))?;
            Ok(Some((Resp::Int(n), after)))
        }
        b'$' => {
            let len = head.parse::<i64>().map_err(|_| format!("bad bulk length {head}"))?;
            if len < 0 {
                return Ok(Some((Resp::Bulk(None), after)));
            }
            let len = len as usize;
            if buf.len() < after + len + 2 {
                return Ok(None);
            }
            if &buf[after + len..after + len + 2] != b"\r\n" {
                return Err("bulk string missing CRLF".into());
            }
            let data = buf[after..after + len].to_vec();
            Ok(Some((Resp::Bulk(Some(data)), after + len + 2)))
        }
        b'*' => {
            let count = head.parse::<i64>().map_err(|_| format!("bad array length {head}"))?;
            if count < 0 {
                return Ok(Some((Resp::Array(Vec::new()), after)));
            }
            let mut items = Vec::with_capacity(count as usize);
            let mut pos = after;
            for _ in 0..count {
                match parse_at(buf, pos)? {
                    Some((value, next)) => {
                        items.push(value);
                        pos = next;
                    }
                    None => return Ok(None),
                }
            }
            Ok(Some((Resp::Array(items), pos)))
        }
        other => Err(format!("unsupported RESP prefix {}", other as char)),
    }
}

fn find_crlf(buf: &[u8], start: usize) -> Option<usize> {
    buf[start..]
        .windows(2)
        .position(|w| w == b"\r\n")
        .map(|p| start + p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_bulk_string_waits_for_crlf() {
        let mut parser = RespParser::new();
        let payload = b"$6\r\nfoobar\r\n";
        parser.push(&payload[..4]);
        assert!(parser.next().unwrap().is_none());
        parser.push(&payload[4..8]);
        assert!(parser.next().unwrap().is_none());
        parser.push(&payload[8..]);
        match parser.next().unwrap() {
            Some(Resp::Bulk(Some(bytes))) => assert_eq!(bytes, b"foobar"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn array_of_bulk_strings() {
        let mut parser = RespParser::new();
        parser.push(b"*2\r\n$9\r\nmaxmemory\r\n$1\r\n0\r\n");
        let value = parser.next().unwrap().unwrap();
        assert_eq!(maxmemory_from_resp(&value).unwrap(), 0);
    }

    #[test]
    fn error_and_simple_string() {
        let mut parser = RespParser::new();
        parser.push(b"+PONG\r\n-NOAUTH Authentication required.\r\n");
        assert_eq!(parser.next().unwrap(), Some(Resp::Simple("PONG".into())));
        assert_eq!(
            parser.next().unwrap(),
            Some(Resp::Error("NOAUTH Authentication required.".into()))
        );
    }

    #[test]
    fn no_password_skips_auth_and_hello() {
        let cmds = handshake_commands(None, None);
        let flat = cmds
            .iter()
            .flat_map(|c| c.iter().cloned())
            .collect::<Vec<_>>();
        assert!(!flat.iter().any(|c| c == "AUTH" || c == "HELLO"));
        let bytes = encode_command(&["PING"]);
        assert_eq!(bytes, b"*1\r\n$4\r\nPING\r\n");
    }

    #[test]
    fn password_sends_auth_without_hello() {
        let cmds = handshake_commands(Some("s3cret"), Some("app"));
        assert_eq!(cmds[0], vec!["AUTH", "app", "s3cret"]);
        assert!(cmds.iter().all(|c| c[0] != "HELLO"));
    }

    #[test]
    fn slowlog_array() {
        let mut parser = RespParser::new();
        parser.push(b"*1\r\n*4\r\n:7\r\n:1700000000\r\n:1500\r\n*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n");
        let value = parser.next().unwrap().unwrap();
        let rows = slowlog_from_resp(&value).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, 7);
        assert_eq!(rows[0].duration_micros, 1500);
        assert_eq!(rows[0].command, "GET key");
    }
}
