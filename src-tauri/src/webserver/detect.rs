use super::config::{parse_apache_config, parse_nginx_config, WebServerKind};
use super::WebServerProbe;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedWebServer {
    pub kind: WebServerKind,
    pub version: Option<String>,
    pub config: Option<super::config::ParsedWebConfig>,
}

pub fn detect_from_probe_outputs(probe: &WebServerProbe) -> Result<Option<DetectedWebServer>, String> {
    let ps = probe.ps_output.to_lowercase();
    let nginx_proc = ps.contains("nginx: master") || ps.split_whitespace().any(|w| w.ends_with("/nginx"));
    let apache_proc = ps.contains("apache2")
        || ps.contains("httpd")
        || ps.contains("/usr/sbin/apache2")
        || ps.contains("/usr/sbin/httpd");

    let nginx_ver = probe
        .nginx_version
        .as_ref()
        .filter(|s| s.to_lowercase().contains("nginx"));
    let apache_ver = probe
        .apache_version
        .as_ref()
        .filter(|s| s.to_lowercase().contains("apache") || s.to_lowercase().contains("httpd"));

    let prefer_nginx = nginx_proc && !apache_proc;
    let prefer_apache = apache_proc && !nginx_proc;

    if prefer_nginx || (nginx_ver.is_some() && apache_ver.is_none()) {
        let config = probe
            .nginx_config
            .as_deref()
            .map(parse_nginx_config);
        return Ok(Some(DetectedWebServer {
            kind: WebServerKind::Nginx,
            version: nginx_ver.map(|s| s.trim().to_string()),
            config,
        }));
    }
    if prefer_apache || (apache_ver.is_some() && nginx_ver.is_none()) {
        let config = probe
            .apache_config
            .as_deref()
            .map(parse_apache_config);
        return Ok(Some(DetectedWebServer {
            kind: WebServerKind::Apache,
            version: apache_ver.map(|s| s.trim().to_string()),
            config,
        }));
    }
    if nginx_proc && apache_proc {
        // Both running: prefer nginx for web traffic unless only apache serves :80
        if nginx_ver.is_some() {
            let config = probe.nginx_config.as_deref().map(parse_nginx_config);
            return Ok(Some(DetectedWebServer {
                kind: WebServerKind::Nginx,
                version: nginx_ver.map(|s| s.trim().to_string()),
                config,
            }));
        }
    }
    Ok(None)
}
