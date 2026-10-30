//! TCP uptime and TLS certificate lifetime. Failures are a down check, not a panic.

use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

fn ping_arguments(host: &str) -> Result<Vec<String>, String> {
    super::notify::validate_host(host)?;
    if host.starts_with('-') {
        return Err("The server address is not valid".into());
    }
    #[cfg(windows)]
    let options = ["-n", "1", "-w", "1000"];
    #[cfg(not(windows))]
    let options = ["-c", "1", "-W", "1"];
    Ok(options
        .iter()
        .map(|s| s.to_string())
        .chain(std::iter::once(host.into()))
        .collect())
}

pub fn probe_ping(host: &str) -> Result<bool, String> {
    let args = ping_arguments(host)?;
    let mut command = std::process::Command::new("ping");
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .output()
        .map(|o| o.status.success())
        .map_err(|_| "The network check could not run".into())
}

pub fn days_until(not_after_unix: i64, now_unix: i64) -> f64 {
    (not_after_unix - now_unix) as f64 / 86_400.0
}

pub fn probe_tcp(host: &str, port: u16) -> Result<f64, String> {
    if host.starts_with("https://") || host.starts_with("http://") {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Could not create HTTP client")?;
        return Ok(
            if client
                .get(host)
                .send()
                .map(|r| r.status().is_success())
                .unwrap_or(false)
            {
                1.
            } else {
                0.
            },
        );
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
    fn ping_builder_passes_one_address_and_rejects_options_and_shell_text() {
        let args = ping_arguments("10.0.0.1").unwrap();
        assert_eq!(args.last().unwrap(), "10.0.0.1");
        assert_eq!(args.len(), 5);
        for host in ["-n", "host & echo secret", "host;whoami", ""] {
            assert!(ping_arguments(host).is_err());
        }
    }

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
