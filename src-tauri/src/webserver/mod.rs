//! Web server detection, config parsing, log parsing, route normalization, and aggregation.
//! Collectors use [`RemoteExec`] so they run over the ConnectionManager's SSH session.

mod aggregate;
mod config;
mod detect;
mod parse;
mod quantile;
mod route;

pub use aggregate::{TrafficAggregator, HEATMAP_MINUTES};
pub use config::{
    ParsedWebConfig, WebServerKind, parse_apache_config, parse_nginx_config,
    suggest_nginx_timing_line, suggest_apache_timing_line,
};
pub use detect::{DetectedWebServer, detect_from_probe_outputs};
pub use parse::{LogFormatKind, parse_access_line};

#[cfg(test)]
mod tests;
pub use route::{RouteNormalizer, RouteRule, validate_rules};

use std::future::Future;
use std::pin::Pin;

/// Run a remote shell command (SSH allowlist) or read a stream chunk.
pub trait RemoteExec {
    type Error: std::error::Error + Send + Sync + 'static;

    fn run_command<'a>(
        &'a self,
        command: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<String, Self::Error>> + Send + 'a>>;
}

/// Probe outputs used for detection (fixtures in tests mirror real commands).
#[derive(Debug, Clone, Default)]
pub struct WebServerProbe {
    pub ps_output: String,
    pub nginx_version: Option<String>,
    pub apache_version: Option<String>,
    pub nginx_config: Option<String>,
    pub apache_config: Option<String>,
}

pub async fn detect_and_parse_config<E: RemoteExec>(
    exec: &E,
) -> Result<Option<DetectedWebServer>, String> {
    let ps = exec.run_command("ps aux").await.map_err(|e| e.to_string())?;
    let nginx_version = match exec.run_command("nginx -v 2>&1").await {
        Ok(v) => Some(v),
        Err(_) => None,
    };
    let apache_version = match exec.run_command("apache2 -v 2>&1 || httpd -v 2>&1").await {
        Ok(v) => Some(v),
        Err(_) => None,
    };
    let probe = WebServerProbe {
        ps_output: ps,
        nginx_version,
        apache_version,
        nginx_config: None,
        apache_config: None,
    };
    let detected = detect_from_probe_outputs(&probe)?;
    if detected.is_none() {
        return Ok(None);
    }
    let mut detected = detected.unwrap();
    match detected.kind {
        WebServerKind::Nginx => {
            let cfg = exec
                .run_command("nginx -T 2>/dev/null || cat /etc/nginx/nginx.conf")
                .await
                .map_err(|e| e.to_string())?;
            detected.config = Some(parse_nginx_config(&cfg));
        }
        WebServerKind::Apache => {
            let cfg = exec
                .run_command("apache2ctl -S 2>/dev/null; cat /etc/apache2/apache2.conf /etc/httpd/conf/httpd.conf 2>/dev/null")
                .await
                .map_err(|e| e.to_string())?;
            detected.config = Some(parse_apache_config(&cfg));
        }
    }
    Ok(Some(detected))
}
