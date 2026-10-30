use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
const OTELCOL_VERSION: &str = "0.128.0";

pub const STEP_ORDER: &[(&str, &str)] = &[
    ("access", "Check access"),
    ("detect", "Look at this server"),
    ("collector", "Install the trace collector"),
    ("web", "Turn on web timing"),
    ("apps", "Connect Node apps"),
    ("logs", "Connect log files"),
    ("companion", "Start the companion"),
    ("verify", "Check that data is flowing"),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub kind: String,
    pub path: String,
    pub backup: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct NodeApp {
    pub name: String,
    pub cwd: String,
    pub pm_id: String,
    pub home: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    pub user: String,
    pub arch: String,
    pub nginx: bool,
    pub apache: bool,
    pub otelcol: String,
    pub nodes: Vec<NodeApp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StepView {
    pub id: String,
    pub title: String,
    pub status: String,
    pub detail: String,
    pub reason: String,
    pub details: String,
    pub script: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Wizard {
    pub steps: Vec<StepView>,
    pub pause: String,
    pub summary: String,
    pub access: String,
    pub detected: Detected,
    pub selected_apps: Vec<String>,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepResult {
    Done(String),
    Skipped(String),
    Failed { reason: String, details: String },
}

impl Wizard {
    pub fn fresh() -> Self {
        Self {
            steps: STEP_ORDER
                .iter()
                .map(|(id, title)| StepView {
                    id: (*id).into(),
                    title: (*title).into(),
                    status: "waiting".into(),
                    detail: String::new(),
                    reason: String::new(),
                    details: String::new(),
                    script: String::new(),
                })
                .collect(),
            pause: "none".into(),
            summary: String::new(),
            access: String::new(),
            detected: Detected::default(),
            selected_apps: Vec::new(),
            changes: Vec::new(),
        }
    }

    pub fn mark(&mut self, id: &str, status: &str, detail: &str, reason: &str, details: &str, script: &str) {
        if let Some(step) = self.steps.iter_mut().find(|step| step.id == id) {
            step.status = status.into();
            if !detail.is_empty() {
                step.detail = detail.into();
            }
            step.reason = reason.into();
            step.details = details.into();
            if !script.is_empty() {
                step.script = script.into();
            }
        }
    }

    pub fn prepare_retry(&mut self) -> Option<String> {
        let id = self.steps.iter().find(|step| step.status == "failed").map(|step| step.id.clone())?;
        let mut seen = false;
        for step in &mut self.steps {
            if step.id == id {
                seen = true;
                step.status = "running".into();
                step.reason.clear();
                step.details.clear();
                continue;
            }
            if seen {
                step.status = "waiting".into();
                step.detail.clear();
                step.reason.clear();
                step.details.clear();
                step.script.clear();
            }
        }
        self.pause = "none".into();
        self.summary.clear();
        Some(id)
    }

    pub fn mark_undone(&mut self) {
        for step in &mut self.steps {
            if matches!(step.status.as_str(), "done" | "failed" | "running") {
                step.status = "skipped".into();
                step.detail = "Restored from the setup record.".into();
                step.reason.clear();
            }
        }
        self.summary = "Setup changes were restored.".into();
        self.pause = "none".into();
        self.changes.clear();
    }

    pub fn add_changes(&mut self, extra: &[Change]) {
        for change in extra {
            if safe_change(change) && !self.changes.iter().any(|row| row.kind == change.kind && row.path == change.path) {
                self.changes.push(change.clone());
            }
        }
    }

    pub fn finished(&self) -> bool {
        self.steps.iter().all(|step| step.status == "done" || step.status == "skipped")
    }
}

pub fn access_script() -> &'static str {
    "echo \"USER $(id -un 2>/dev/null || echo unknown)\"\nif [ \"$(id -u)\" = 0 ]; then echo ACCESS root; exit 0; fi\nif sudo -n true 2>/dev/null; then echo ACCESS nopass; exit 0; fi\necho ACCESS password\nexit 0\n"
}

pub fn parse_access(text: &str) -> (String, String) {
    let mut user = String::new();
    let mut access = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("USER ") {
            user = name.trim().to_string();
        } else if let Some(kind) = line.strip_prefix("ACCESS ") {
            access = kind.trim().to_string();
        }
    }
    (user, access)
}

pub fn detect_script() -> &'static str {
    r#"os=unknown
if [ -r /etc/os-release ]; then os=$(. /etc/os-release 2>/dev/null && printf '%s' "${ID:-unknown}"); fi
echo "OS $os"
echo "ARCH $(uname -m 2>/dev/null || echo unknown)"
if command -v nginx >/dev/null 2>&1 || [ -d /etc/nginx ]; then echo WEB nginx; fi
if command -v apache2ctl >/dev/null 2>&1 || command -v apachectl >/dev/null 2>&1 || command -v httpd >/dev/null 2>&1; then echo WEB apache; fi
if command -v otelcol-contrib >/dev/null 2>&1 || [ -f /etc/otelcol-contrib/config.yaml ]; then echo OTELCOL present; else echo OTELCOL missing; fi
echo "USER $(id -un 2>/dev/null || echo unknown)"
for home in /root/.pm2 /home/*/.pm2; do
  [ -d "$home" ] || continue
  echo "PM2HOME $home"
  echo PM2JSON
  PM2_HOME="$home" pm2 jlist 2>/dev/null || true
  echo PM2END
done
if command -v pm2 >/dev/null 2>&1; then
  echo PM2HOME
  echo PM2JSON
  pm2 jlist 2>/dev/null || true
  echo PM2END
fi
exit 0
"#
}

pub fn parse_detect(text: &str) -> Detected {
    let mut detected = Detected::default();
    let mut home = String::new();
    let mut json = String::new();
    let mut in_json = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if in_json {
            if trimmed == "PM2END" {
                detected.nodes.extend(parse_pm2_blob(&json, &home));
                json.clear();
                in_json = false;
            } else {
                json.push_str(line);
                json.push('\n');
            }
            continue;
        }
        if let Some(path) = trimmed.strip_prefix("PM2HOME") {
            home = path.trim().to_string();
            continue;
        }
        if trimmed == "PM2JSON" {
            in_json = true;
            json.clear();
            continue;
        }
        if let Some(arch) = trimmed.strip_prefix("ARCH ") {
            detected.arch = arch.trim().to_string();
        } else if trimmed == "WEB nginx" {
            detected.nginx = true;
        } else if trimmed == "WEB apache" {
            detected.apache = true;
        } else if let Some(state) = trimmed.strip_prefix("OTELCOL ") {
            detected.otelcol = state.trim().to_string();
        } else if let Some(user) = trimmed.strip_prefix("USER ") {
            detected.user = user.trim().to_string();
        }
    }
    detected.nodes.sort_by(|a, b| a.name.cmp(&b.name));
    detected.nodes.dedup_by(|a, b| a.name == b.name && a.cwd == b.cwd);
    detected
}

fn parse_pm2_blob(text: &str, home: &str) -> Vec<NodeApp> {
    let Ok(value) = serde_json::from_str::<Value>(text.trim()) else {
        return Vec::new();
    };
    let Some(rows) = value.as_array() else {
        return Vec::new();
    };
    let mut apps = Vec::new();
    for row in rows {
        let name = row.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let env = row.get("pm2_env");
        let cwd = env.and_then(|v| v.get("pm_cwd")).and_then(|v| v.as_str()).unwrap_or("");
        let pm_id = row
            .get("pm_id")
            .and_then(|v| v.as_u64().map(|n| n.to_string()).or_else(|| v.as_str().map(|s| s.to_string())))
            .unwrap_or_default();
        if !safe_token(name) || crate::insight::detect::validate_log_path(cwd).is_err() || !pm_id.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let interpreter = env.and_then(|v| v.get("exec_interpreter")).and_then(|v| v.as_str()).unwrap_or("node");
        if !interpreter.contains("node") && env.and_then(|v| v.get("exec_mode")).is_none() {
            continue;
        }
        apps.push(NodeApp {
            name: name.into(),
            cwd: cwd.into(),
            pm_id,
            home: if crate::insight::detect::validate_log_path(home).is_ok() { home.into() } else { String::new() },
        });
    }
    apps
}

pub fn deb_arch(uname: &str) -> Option<&'static str> {
    match uname {
        "x86_64" => Some("amd64"),
        "aarch64" => Some("arm64"),
        "armv7l" => Some("armv7"),
        "i686" | "i386" => Some("386"),
        "ppc64le" => Some("ppc64le"),
        _ => None,
    }
}

pub fn collector_config() -> &'static str {
    "receivers:\n  otlp:\n    protocols:\n      grpc:\n        endpoint: 127.0.0.1:4317\n      http:\n        endpoint: 127.0.0.1:4318\nexporters:\n  file:\n    path: /var/log/otel/telemetry.json\n    rotation:\n      max_megabytes: 20\n      max_days: 7\n      max_backups: 5\nprocessors:\n  batch:\nservice:\n  pipelines:\n    traces:\n      receivers: [otlp]\n      processors: [batch]\n      exporters: [file]\n    logs:\n      receivers: [otlp]\n      processors: [batch]\n      exporters: [file]\n    metrics:\n      receivers: [otlp]\n      processors: [batch]\n      exporters: [file]\n"
}

pub fn collector_script(user: &str, arch: &str) -> String {
    let user = if crate::setup::safe_user(user) == user { user } else { "" };
    let arch = deb_arch(arch).unwrap_or("");
    let config = collector_config();
    format!(
        r#"user={user}
arch={arch}
cfg=/etc/otelcol-contrib/config.yaml
bak=/etc/tracelyn/backups/otelcol-config.yaml
drop=/etc/systemd/system/otelcol-contrib.service.d/tracelyn.conf
ver={ver}
if [ -z "$arch" ]; then echo FAIL This server architecture is not supported for the collector.; exit 1; fi
mkdir -p /etc/tracelyn/backups /var/log/otel /etc/systemd/system/otelcol-contrib.service.d
tmp=$(mktemp)
cat > "$tmp" << 'EOF'
{config}EOF
if ! command -v otelcol-contrib >/dev/null 2>&1; then
  url="https://github.com/open-telemetry/opentelemetry-collector-releases/releases/download/v$ver/otelcol-contrib_${{ver}}_linux_${{arch}}"
  pkg=$(mktemp)
  if command -v dpkg >/dev/null 2>&1; then
    if command -v curl >/dev/null 2>&1; then curl -fsSL -o "$pkg" "${{url}}.deb" || {{ echo FAIL The collector package could not be downloaded.; exit 1; }}
    elif command -v wget >/dev/null 2>&1; then wget -q -O "$pkg" "${{url}}.deb" || {{ echo FAIL The collector package could not be downloaded.; exit 1; }}
    else echo FAIL This server has no curl or wget, so the collector could not be downloaded.; exit 1; fi
    dpkg -i "$pkg" || apt-get install -f -y || {{ echo FAIL The collector package did not install.; exit 1; }}
    echo MANIFEST package otelcol-contrib
  elif command -v rpm >/dev/null 2>&1; then
    if command -v curl >/dev/null 2>&1; then curl -fsSL -o "$pkg" "${{url}}.rpm" || {{ echo FAIL The collector package could not be downloaded.; exit 1; }}
    elif command -v wget >/dev/null 2>&1; then wget -q -O "$pkg" "${{url}}.rpm" || {{ echo FAIL The collector package could not be downloaded.; exit 1; }}
    else echo FAIL This server has no curl or wget, so the collector could not be downloaded.; exit 1; fi
    if command -v dnf >/dev/null 2>&1; then dnf install -y "$pkg" || {{ echo FAIL The collector package did not install.; exit 1; }}
    elif command -v yum >/dev/null 2>&1; then yum install -y "$pkg" || {{ echo FAIL The collector package did not install.; exit 1; }}
    else rpm -Uvh "$pkg" || {{ echo FAIL The collector package did not install.; exit 1; }}; fi
    echo MANIFEST package otelcol-contrib
  else
    echo FAIL No supported package manager was found.
    exit 1
  fi
  rm -f "$pkg"
fi
if [ -f "$cfg" ] && cmp -s "$cfg" "$tmp"; then
  echo SKIP config
else
  if [ -f "$cfg" ]; then cp -a "$cfg" "$bak"; echo "MANIFEST file $cfg $bak"; else echo "MANIFEST created $cfg"; fi
  mkdir -p /etc/otelcol-contrib
  cat "$tmp" > "$cfg"
fi
rm -f "$tmp"
cat > "$drop" << 'EOF'
[Service]
UMask=0022
EOF
echo "MANIFEST created $drop"
groupadd -f tracelyn 2>/dev/null || true
if [ -n "$user" ]; then usermod -aG tracelyn "$user" 2>/dev/null || true; fi
id otelcol-contrib >/dev/null 2>&1 && usermod -aG tracelyn otelcol-contrib 2>/dev/null || true
chgrp tracelyn /var/log/otel 2>/dev/null || true
chmod 2755 /var/log/otel 2>/dev/null || true
touch /var/log/otel/telemetry.json
if [ -n "$user" ] && command -v setfacl >/dev/null 2>&1; then
  setfacl -m "u:${{user}}:rX" /var/log/otel || true
  setfacl -d -m "u:${{user}}:r" /var/log/otel || true
  setfacl -m "u:${{user}}:r" /var/log/otel/telemetry.json || true
  echo "MANIFEST acl /var/log/otel $user"
fi
chmod a+r /var/log/otel/telemetry.json 2>/dev/null || true
plain="$cfg.plain"
grep -v 'rotation:' -A 3 "$cfg" > "$plain" || cp "$cfg" "$plain"
if otelcol-contrib validate --config "$cfg" 2>/dev/null || otelcol-contrib --config "$cfg" validate 2>/dev/null; then
  true
elif otelcol-contrib validate --config "$plain" 2>/dev/null || otelcol-contrib --config "$plain" validate 2>/dev/null; then
  cat "$plain" > "$cfg"
else
  if [ -f "$bak" ]; then cp -a "$bak" "$cfg"; elif [ ! -f "$bak" ]; then rm -f "$cfg"; fi
  echo FAIL The collector config was rejected, so the previous file was restored.
  exit 1
fi
rm -f "$plain"
systemctl daemon-reload 2>/dev/null || true
systemctl enable otelcol-contrib 2>/dev/null || true
systemctl restart otelcol-contrib 2>/dev/null || service otelcol-contrib restart 2>/dev/null || true
if ! systemctl is-active --quiet otelcol-contrib 2>/dev/null && ! pgrep -f otelcol-contrib >/dev/null 2>&1; then
  if [ -f "$bak" ]; then cp -a "$bak" "$cfg"; systemctl restart otelcol-contrib 2>/dev/null || true; fi
  echo FAIL The collector did not stay running, so the previous config was restored.
  exit 1
fi
echo MANIFEST unit otelcol-contrib
chmod a+r /var/log/otel/telemetry.json 2>/dev/null || true
echo OK collector
"#,
        user = user,
        arch = arch,
        ver = OTELCOL_VERSION,
        config = config,
    )
}

pub fn nginx_timing_conf() -> &'static str {
    "log_format tracelyn '$remote_addr - $remote_user [$time_local] \"$request\" $status $body_bytes_sent rt=$request_time urt=$upstream_response_time';\naccess_log /var/log/nginx/tracelyn-access.log tracelyn;\n"
}

pub fn apache_timing_conf() -> &'static str {
    "LogFormat \"%h %l %u %t \\\"%r\\\" %>s %b rt=%D\" tracelyn\nCustomLog /var/log/apache2/tracelyn-access.log tracelyn\n"
}

pub fn web_script(nginx: bool, apache: bool) -> String {
    let mut script = String::from("mkdir -p /etc/tracelyn/backups\n");
    if nginx {
        script.push_str(&format!(
            "conf=/etc/nginx/conf.d/tracelyn-timing.conf\nbak=/etc/tracelyn/backups/nginx-timing.conf\nmkdir -p /etc/nginx/conf.d\nif [ -f \"$conf\" ]; then cp -a \"$conf\" \"$bak\"; echo \"MANIFEST file $conf $bak\"; else echo \"MANIFEST created $conf\"; fi\ncat > \"$conf\" << 'EOF'\n{}EOF\nif nginx -t 2>/tmp/tracelyn-nginx.err; then nginx -s reload 2>/dev/null || systemctl reload nginx 2>/dev/null || true; echo OK nginx; else if [ -f \"$bak\" ]; then cp -a \"$bak\" \"$conf\"; else rm -f \"$conf\"; fi; echo FAIL Web server config was rejected, so the previous file was restored.; cat /tmp/tracelyn-nginx.err; exit 1; fi\n",
            nginx_timing_conf()
        ));
    }
    if apache {
        script.push_str(&format!(
            "aconf=/etc/apache2/conf-available/tracelyn-timing.conf\nif [ ! -d /etc/apache2/conf-available ] && [ -d /etc/httpd/conf.d ]; then aconf=/etc/httpd/conf.d/tracelyn-timing.conf; fi\nabak=/etc/tracelyn/backups/apache-timing.conf\nmkdir -p \"$(dirname \"$aconf\")\"\nif [ -f \"$aconf\" ]; then cp -a \"$aconf\" \"$abak\"; echo \"MANIFEST file $aconf $abak\"; else echo \"MANIFEST created $aconf\"; fi\ncat > \"$aconf\" << 'EOF'\n{}EOF\nif command -v a2enconf >/dev/null 2>&1; then a2enconf tracelyn-timing >/dev/null 2>&1 || true; fi\nif apachectl configtest >/tmp/tracelyn-apache.err 2>&1 || apache2ctl configtest >/tmp/tracelyn-apache.err 2>&1; then systemctl reload apache2 2>/dev/null || systemctl reload httpd 2>/dev/null || apachectl graceful 2>/dev/null || true; echo OK apache; else if [ -f \"$abak\" ]; then cp -a \"$abak\" \"$aconf\"; else rm -f \"$aconf\"; fi; echo FAIL Apache config was rejected, so the previous file was restored.; cat /tmp/tracelyn-apache.err; exit 1; fi\n",
            apache_timing_conf()
        ));
    }
    if !nginx && !apache {
        script.push_str("echo SKIP No web server was found, so timing logs were left unchanged.\n");
    }
    script.push_str("exit 0\n");
    script
}

pub fn node_script(apps: &[NodeApp]) -> String {
    let mut script = String::from("ok=0\n");
    for app in apps {
        if !safe_token(&app.name) || !app.pm_id.chars().all(|c| c.is_ascii_digit()) || crate::insight::detect::validate_log_path(&app.cwd).is_err() {
            continue;
        }
        let home = if crate::insight::detect::validate_log_path(&app.home).is_ok() {
            format!("export PM2_HOME='{}'\n", app.home)
        } else {
            String::new()
        };
        script.push_str(&format!(
            r#"{home}cd '{cwd}' || {{ echo FAIL Could not open the app folder for {name}.; exit 1; }}
if [ ! -f package.json ]; then echo "SKIP {name} is not a Node app Tracelyn can instrument automatically."; else
  mkdir -p /etc/tracelyn/backups
  cp -a package.json '/etc/tracelyn/backups/pkg-{name}.json'
  echo "MANIFEST file {cwd}/package.json /etc/tracelyn/backups/pkg-{name}.json"
  npm install --save --no-audit --no-fund @opentelemetry/auto-instrumentations-node @opentelemetry/api || {{ echo FAIL Could not install the OpenTelemetry packages for {name}.; exit 1; }}
  export NODE_OPTIONS="--require @opentelemetry/auto-instrumentations-node/register"
  export OTEL_SERVICE_NAME='{name}'
  export OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318
  export OTEL_TRACES_EXPORTER=otlp
  export OTEL_LOGS_EXPORTER=otlp
  export OTEL_METRICS_EXPORTER=otlp
  pm2 restart {pm} --update-env || {{ echo FAIL Could not restart {name}.; exit 1; }}
  pm2 save || true
  echo "OK {name}"
  ok=1
fi
"#,
            home = home,
            cwd = app.cwd,
            name = app.name,
            pm = app.pm_id,
        ));
    }
    script.push_str("if [ \"$ok\" = 1 ]; then echo OK apps; else echo SKIP No Node app was changed. Apps that are not run by PM2 are left as they are.; fi\nexit 0\n");
    script
}

pub fn log_probe_script() -> &'static str {
    r#"for p in /var/log/syslog /var/log/messages /var/log/nginx/error.log /var/log/nginx/access.log /var/log/apache2/error.log /var/log/apache2/access.log /var/log/httpd/error_log /var/log/otel/telemetry.json; do
  parent=$(dirname -- "$p")
  if [ -e "$p" ] || { [ -d "$parent" ] && [ ! -x "$parent" ]; }; then printf 'HOSTLOG %s\n' "$p"; fi
done
exit 0
"#
}

pub fn verify_script() -> String {
    format!(
        r#"tid={tid}
body='{{"resourceSpans":[{{"resource":{{"attributes":[{{"key":"service.name","value":{{"stringValue":"tracelyn-setup"}}}}]}},"scopeSpans":[{{"spans":[{{"traceId":"'{tid}'","spanId":"00f067aa0ba902b7","name":"setup-check","kind":1,"startTimeUnixNano":"1700000000000000000","endTimeUnixNano":"1700000000001000000"}}]}}]}}]}}'
if command -v curl >/dev/null 2>&1; then curl -sS -m 8 -X POST http://127.0.0.1:4318/v1/traces -H 'Content-Type: application/json' --data "$body" >/dev/null || true; fi
sleep 2
hit=$(grep -m 1 -F "$tid" /var/log/otel/telemetry.json 2>/dev/null || true)
if [ -n "$hit" ]; then echo VERIFY_OK; printf '%s\n' "$hit" | head -c 4000; echo; else echo VERIFY_FAIL; fi
if [ -r /var/log/syslog ] || [ -r /var/log/messages ] || [ -r /var/log/nginx/error.log ] || [ -r /var/log/nginx/access.log ] || [ -r /var/log/apache2/error.log ] || [ -r /var/log/otel/telemetry.json ]; then echo LOG_OK; else echo LOG_FAIL; fi
exit 0
"#,
        tid = TRACE_ID,
    )
}

pub fn manifest_script(changes: &[Change]) -> String {
    let json = serde_json::to_string(&changes).unwrap_or_else(|_| "[]".into());
    format!("mkdir -p /etc/tracelyn\ncat > /etc/tracelyn/setup-manifest.json << 'EOF'\n{json}\nEOF\necho OK manifest\n")
}

pub fn read_manifest_script() -> &'static str {
    "cat /etc/tracelyn/setup-manifest.json 2>/dev/null || true\n"
}

pub fn undo_script(changes: &[Change]) -> String {
    let mut lines = vec!["set -u".to_string(), "mkdir -p /etc/tracelyn/backups".into()];
    for change in changes.iter().rev() {
        if !safe_change(change) {
            continue;
        }
        match change.kind.as_str() {
            "file" => lines.push(format!(
                "if [ -f '{}' ]; then cp -a '{}' '{}'; fi",
                change.backup, change.backup, change.path
            )),
            "created" => lines.push(format!("rm -f '{}'", change.path)),
            "package" => lines.push(format!(
                "if command -v apt-get >/dev/null 2>&1; then apt-get remove -y '{}'; elif command -v dnf >/dev/null 2>&1; then dnf remove -y '{}'; elif command -v yum >/dev/null 2>&1; then yum remove -y '{}'; fi",
                change.path, change.path, change.path
            )),
            "unit" => lines.push(format!(
                "systemctl disable --now '{}' 2>/dev/null || true",
                change.path
            )),
            "acl" => lines.push(format!(
                "setfacl -x u:{} '{}' 2>/dev/null || true",
                change.backup, change.path
            )),
            _ => {}
        }
    }
    lines.push("systemctl daemon-reload 2>/dev/null || true".into());
    lines.push("nginx -t >/dev/null 2>&1 && nginx -s reload >/dev/null 2>&1 || true".into());
    lines.push("apachectl configtest >/dev/null 2>&1 && apachectl graceful >/dev/null 2>&1 || true".into());
    lines.push("rm -f /etc/tracelyn/setup-manifest.json".into());
    lines.push("echo OK undo".into());
    lines.join("\n")
}

pub fn changes_from_output(text: &str) -> Vec<Change> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("MANIFEST ") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let change = Change {
            kind: parts.next().unwrap_or("").into(),
            path: parts.next().unwrap_or("").into(),
            backup: parts.next().unwrap_or("").into(),
        };
        if safe_change(&change) {
            out.push(change);
        }
    }
    out
}

pub fn interpret(stdout: &str, stderr: &str, code: i32) -> StepResult {
    let fail = stdout.lines().find_map(|line| line.trim().strip_prefix("FAIL ").map(|s| s.to_string()));
    let skip = stdout.lines().find_map(|line| line.trim().strip_prefix("SKIP ").map(|s| s.to_string()));
    if let Some(reason) = fail {
        return StepResult::Failed {
            reason: plain(&reason),
            details: details(stdout, stderr),
        };
    }
    if code != 0 {
        return StepResult::Failed {
            reason: "This step did not finish.".into(),
            details: details(stdout, stderr),
        };
    }
    if let Some(reason) = skip {
        let detail = if stdout.lines().any(|line| line.trim().starts_with("OK ")) {
            stdout.lines().filter(|line| line.trim().starts_with("OK ")).collect::<Vec<_>>().join(" ")
        } else {
            reason.clone()
        };
        if stdout.lines().any(|line| line.trim().starts_with("OK ")) {
            return StepResult::Done(plain(&detail));
        }
        return StepResult::Skipped(plain(&reason));
    }
    let detail = stdout
        .lines()
        .filter(|line| line.trim().starts_with("OK "))
        .map(|line| line.trim().trim_start_matches("OK ").to_string())
        .collect::<Vec<_>>()
        .join(" ");
    StepResult::Done(if detail.is_empty() { "Finished.".into() } else { plain(&detail) })
}

fn details(stdout: &str, stderr: &str) -> String {
    let mut text = String::new();
    if !stderr.trim().is_empty() {
        text.push_str(stderr.trim());
    }
    if !stdout.trim().is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(stdout.trim());
    }
    let masked = mask_private(&text);
    masked.chars().take(1500).collect()
}

fn plain(text: &str) -> String {
    text.replace(';', ",").chars().take(400).collect()
}

pub fn mask_private(text: &str) -> String {
    text.lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if lower.contains("password") || lower.contains("passphrase") || lower.contains("sudo -s") && lower.contains("hunter") {
                return "[secret]".into();
            }
            if lower.contains("password") || lower.contains("passphrase") {
                return "[secret]".into();
            }
            mask_line(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn mask_line(line: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        if rest.starts_with("/root/.ssh") || rest.starts_with("/home/") && rest.contains("/.ssh") {
            if rest.starts_with("/root/.ssh") {
                out.push_str("[key]");
                i += "/root/.ssh".chars().count();
                while i < chars.len() && chars[i] != ' ' && chars[i] != '\t' {
                    i += 1;
                }
                continue;
            }
        }
        if rest.starts_with("/home/") {
            out.push_str("/home/[user]");
            i += "/home/".chars().count();
            while i < chars.len() && chars[i] != '/' && chars[i] != ' ' && chars[i] != '\t' {
                i += 1;
            }
            continue;
        }
        if ipv4_at(&chars, i) {
            out.push_str("[address]");
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn ipv4_at(chars: &[char], start: usize) -> bool {
    let mut groups = 0;
    let mut digits = 0;
    let mut i = start;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            digits += 1;
            if digits > 3 {
                return false;
            }
            i += 1;
            continue;
        }
        if chars[i] == '.' && digits > 0 {
            groups += 1;
            digits = 0;
            i += 1;
            if groups > 3 {
                return false;
            }
            continue;
        }
        break;
    }
    groups == 3 && digits > 0
}

fn safe_token(value: &str) -> bool {
    !value.is_empty() && value.len() < 80 && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn safe_change(change: &Change) -> bool {
    let ok = |value: &str| {
        !value.is_empty()
            && value.len() < 240
            && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':' ))
            && !value.contains("..")
    };
    match change.kind.as_str() {
        "package" | "unit" => safe_token(&change.path),
        "acl" => change.path.starts_with('/') && ok(&change.path) && safe_token(&change.backup),
        "created" => change.path.starts_with('/') && ok(&change.path),
        "file" => change.path.starts_with('/') && change.backup.starts_with('/') && ok(&change.path) && ok(&change.backup),
        _ => false,
    }
}

pub fn summary_for(detected: &Detected, selected: &[String]) -> String {
    if let Some(name) = selected.iter().find(|name| detected.nodes.iter().any(|app| &app.name == *name)) {
        return format!("Traces are flowing from {name}.");
    }
    if let Some(name) = detected.nodes.first() {
        return format!("Traces are flowing from {}.", name.name);
    }
    "Traces are flowing.".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_retries_only_the_failed_step() {
        let mut wizard = Wizard::fresh();
        wizard.mark("access", "done", "Signed in.", "", "", "");
        wizard.mark("detect", "failed", "", "Could not read the server.", "permission denied", "echo detect");
        assert_eq!(wizard.prepare_retry().as_deref(), Some("detect"));
        assert_eq!(wizard.steps[0].status, "done");
        assert_eq!(wizard.steps[1].status, "running");
        assert_eq!(wizard.steps[2].status, "waiting");
        wizard.mark("detect", "done", "Found nginx.", "", "", "");
        wizard.mark("collector", "done", "Collector installed.", "", "", "");
        wizard.mark_undone();
        assert!(wizard.steps.iter().all(|step| step.status == "skipped" || step.status == "waiting"));
        assert_eq!(wizard.summary, "Setup changes were restored.");
    }

    #[test]
    fn sudo_wrapper_and_nginx_and_rollback() {
        let script = collector_script("deploy", "x86_64");
        assert!(script.contains("otelcol-contrib validate --config"));
        assert!(script.contains("/var/log/otel/telemetry.json"));
        assert!(script.contains("setfacl"));
        assert!(script.contains("deploy"));
        assert!(!script.contains("sudo -S"));
        assert!(nginx_timing_conf().contains("request_time"));
        assert!(nginx_timing_conf().contains("upstream_response_time"));
        let web = web_script(true, false);
        assert!(web.contains("nginx -t"));
        assert!(web.contains("cp -a"));
        let changes = vec![
            Change { kind: "created".into(), path: "/etc/nginx/conf.d/tracelyn-timing.conf".into(), backup: String::new() },
            Change { kind: "file".into(), path: "/etc/otelcol-contrib/config.yaml".into(), backup: "/etc/tracelyn/backups/otelcol-config.yaml".into() },
        ];
        let undo = undo_script(&changes);
        let file_at = undo.find("otelcol-config.yaml").unwrap();
        let created_at = undo.find("tracelyn-timing.conf").unwrap();
        assert!(file_at < created_at);
        assert!(undo.contains("rm -f '/etc/nginx/conf.d/tracelyn-timing.conf'"));
    }

    #[test]
    fn parses_detection_and_manifest_lines() {
        let text = "\
ARCH x86_64
WEB nginx
OTELCOL missing
USER deploy
PM2HOME /root/.pm2
PM2JSON
[{\"name\":\"bharattales-web\",\"pm_id\":0,\"pm2_env\":{\"pm_cwd\":\"/var/www/node-application/bharattales-web\",\"exec_interpreter\":\"node\"}}]
PM2END
";
        let found = parse_detect(&text);
        assert_eq!(found.arch, "x86_64");
        assert!(found.nginx);
        assert_eq!(found.nodes.len(), 1);
        assert_eq!(found.nodes[0].name, "bharattales-web");
        assert_eq!(deb_arch("x86_64"), Some("amd64"));
        let nodes = node_script(&found.nodes);
        assert!(nodes.contains("OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318"));
        assert!(nodes.contains("--update-env"));
        let breakout = format!("{}{}", "';", " rm");
        assert!(!nodes.contains(&breakout));
        let lines = "MANIFEST file /etc/otelcol-contrib/config.yaml /etc/tracelyn/backups/otelcol-config.yaml\nMANIFEST package otelcol-contrib\nMANIFEST bad $(rm)\n";
        let changes = changes_from_output(lines);
        assert_eq!(changes.len(), 2);
        assert!(matches!(interpret("OK collector\n", "", 0), StepResult::Done(_)));
        assert!(matches!(interpret("SKIP No web server was found, so timing logs were left unchanged.\n", "", 0), StepResult::Skipped(_)));
        assert!(matches!(interpret("FAIL The collector config was rejected, so the previous file was restored.\n", "stderr", 1), StepResult::Failed { .. }));
    }

    #[test]
    fn masks_addresses_users_and_secrets() {
        let masked = mask_private("denied 10.1.2.3 for /home/deploy/app");
        assert!(masked.contains("[address]"));
        assert!(masked.contains("/home/[user]"));
        assert!(!masked.contains("10.1.2.3"));
        assert!(!masked.contains("deploy"));
        assert_eq!(mask_private("password=hunter2"), "[secret]");
        assert!(mask_private("open /root/.ssh/id_rsa").contains("[key]"));
    }

    #[test]
    fn access_parser_and_verify_trace_id() {
        let (user, access) = parse_access("USER deploy\nACCESS password\n");
        assert_eq!(user, "deploy");
        assert_eq!(access, "password");
        assert!(verify_script().contains(TRACE_ID));
        assert!(log_probe_script().contains("/var/log/otel/telemetry.json"));
    }
}
