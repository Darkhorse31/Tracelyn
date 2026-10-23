use crate::webserver::{
    detect_from_probe_outputs, normalize_path, parse_access_line, parse_apache_config,
    parse_apache_timing_line, parse_nginx_config, AccessLogEntry, LogFormatKind, RouteRule,
    WebServerProbe,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseLogLineInput {
    pub line: String,
    pub format: LogFormatKind,
    pub apache_timing: Option<bool>,
}

#[tauri::command]
pub fn web_parse_log_line(input: ParseLogLineInput) -> Result<AccessLogEntry, String> {
    if input.apache_timing == Some(true) {
        return parse_apache_timing_line(&input.line).map_err(|e| e.message);
    }
    parse_access_line(&input.line, input.format).map_err(|e| e.message)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectWebserverInput {
    pub ps_output: String,
    pub nginx_version: Option<String>,
    pub apache_version: Option<String>,
    pub nginx_config: Option<String>,
    pub apache_config: Option<String>,
}

#[tauri::command]
pub fn web_detect_server(input: DetectWebserverInput) -> Result<Option<crate::webserver::DetectedWebServer>, String> {
    let probe = WebServerProbe {
        ps_output: input.ps_output,
        nginx_version: input.nginx_version,
        apache_version: input.apache_version,
        nginx_config: input.nginx_config,
        apache_config: input.apache_config,
    };
    detect_from_probe_outputs(&probe)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseConfigInput {
    pub kind: String,
    pub text: String,
}

#[tauri::command]
pub fn web_parse_config(input: ParseConfigInput) -> Result<serde_json::Value, String> {
    let value = match input.kind.as_str() {
        "nginx" => serde_json::to_value(parse_nginx_config(&input.text)).map_err(|e| e.to_string())?,
        "apache" => serde_json::to_value(parse_apache_config(&input.text)).map_err(|e| e.to_string())?,
        _ => return Err("kind must be nginx or apache".into()),
    };
    Ok(value)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizeRouteInput {
    pub path: String,
    pub rules: Vec<RouteRule>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizeRouteOutput {
    pub normalized: String,
}

#[tauri::command]
pub fn web_normalize_route(input: NormalizeRouteInput) -> NormalizeRouteOutput {
    NormalizeRouteOutput {
        normalized: normalize_path(&input.path, &input.rules),
    }
}
