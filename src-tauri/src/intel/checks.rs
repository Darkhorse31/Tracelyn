//! TCP uptime and TLS certificate lifetime. Failures are a down check, not a panic.

use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

pub fn days_until(not_after_unix: i64, now_unix: i64) -> f64 {
    (not_after_unix - now_unix) as f64 / 86_400.0
}

pub fn probe_tcp(host: &str, port: u16) -> Result<f64, String> {
    if host.starts_with("https://")||host.starts_with("http://") {
        let client=reqwest::blocking::Client::builder().timeout(Duration::from_secs(8)).redirect(reqwest::redirect::Policy::none()).build().map_err(|_|"Could not create HTTP client")?;
        return Ok(if client.get(host).send().map(|r|r.status().is_success()).unwrap_or(false){1.}else{0.});
    }
    super::notify::validate_host(host)?;
    if port == 0 {
        return Err("port is required".into());
    }
    let addr = format!("{host}:{port}");
    let socket = match addr.to_socket_addrs() {
        Ok(mut iter) => iter.next(),
        Err(_) => return Ok(0.0),
    };
    let Some(socket) = socket else {
        return Ok(0.0);
    };
    match TcpStream::connect_timeout(&socket, Duration::from_secs(5)) {
        Ok(mut stream) => {
            let _ = stream.write(&[]);
            Ok(1.0)
        }
        Err(_) => Ok(0.0),
    }
}

pub fn probe_ssl_days(host: &str, port: u16, now_ms: i64) -> Result<f64, String> {
    super::notify::validate_host(host)?;
    let port = if port == 0 { 443 } else { port };
    let addr = format!("{host}:{port}");
    let socket = addr
        .to_socket_addrs()
        .map_err(|_| format!("could not resolve {host}"))?
        .next()
        .ok_or_else(|| format!("could not resolve {host}"))?;
    let tcp = TcpStream::connect_timeout(&socket, Duration::from_secs(8))
        .map_err(|e| format!("TLS connect: {e}"))?;
    tcp.set_read_timeout(Some(Duration::from_secs(8))).ok();
    let connector = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| e.to_string())?;
    let stream = connector
        .connect(host, tcp)
        .map_err(|e| format!("TLS handshake: {e}"))?;
    let cert = stream
        .peer_certificate()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "server did not present a certificate".to_string())?;
    let der = cert.to_der().map_err(|e| e.to_string())?;
    let (_, parsed) = x509_parser::parse_x509_certificate(&der).map_err(|e| e.to_string())?;
    let not_after = parsed.validity().not_after.timestamp();
    Ok(days_until(not_after, now_ms / 1000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_until_is_signed() {
        assert!((days_until(86_400, 0) - 1.0).abs() < 1e-9);
        assert!(days_until(0, 86_400) < 0.0);
    }

    #[test]
    fn tcp_probe_rejects_a_bad_host() {
        assert!(probe_tcp("bad host", 80).is_err());
    }
}
