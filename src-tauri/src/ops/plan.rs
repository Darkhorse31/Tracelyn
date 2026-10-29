//! Pure planning for guarded operations. No SSH, no database.

const EXCERPT_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanInput {
    pub kind: String,
    pub verb: String,
    pub target: String,
    pub ssh_user: String,
    pub pm2_user: Option<String>,
    pub pm2_home: Option<String>,
    pub log_paths: Vec<String>,
    pub use_sudo: bool,
    pub sudo_password_set: bool,
    pub confirm_name: Option<String>,
    pub health_timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedStep {
    pub command: String,
    pub skip_if_prev_failed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedAction {
    pub steps: Vec<PlannedStep>,
    pub health_command: String,
    pub dangerous: bool,
    pub confirm_name: String,
    pub target: String,
    pub kind: String,
    pub verb: String,
    pub pm2_user: Option<String>,
    pub pm2_home: Option<String>,
    pub use_sudo: bool,
    pub health_timeout_secs: u64,
}

impl PlannedAction {
    pub fn display(&self) -> String {
        self.steps
            .iter()
            .map(|step| step.command.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn step_should_run(prev_failed: bool, skip_if_prev_failed: bool) -> bool {
    !(prev_failed && skip_if_prev_failed)
}

pub fn clamp_health_timeout(secs: Option<u64>) -> u64 {
    secs.unwrap_or(15).clamp(1, 60)
}

pub fn needs_sudo_stdin(command: &str) -> bool {
    command.starts_with("sudo -S ")
}

pub fn redact(output: &str, secret: Option<&str>) -> String {
    let mut text = output.replace('\0', "");
    if let Some(secret) = secret {
        if secret.len() >= 4 && text.contains(secret) {
            text = text.replace(secret, "••••");
        }
    }
    text
}

pub fn excerpt(text: &str) -> String {
    if text.chars().count() <= EXCERPT_CHARS {
        return text.to_string();
    }
    let mut out: String = text.chars().take(EXCERPT_CHARS).collect();
    out.push('…');
    out
}

pub fn actor_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "unknown".into())
}

/// `(user, PM2_HOME)` from a log path such as `/home/deploy/.pm2/logs/api-out.log`.
pub fn infer_pm2(paths: &[String]) -> Option<(String, String)> {
    for path in paths {
        let idx = if let Some(found) = path.find("/.pm2/") {
            found
        } else if let Some(stripped) = path.strip_suffix("/.pm2") {
            stripped.len()
        } else {
            continue;
        };
        let prefix = &path[..idx];
        if prefix.is_empty() || prefix.contains("..") {
            continue;
        }
        let user = if prefix == "/root" {
            "root".to_string()
        } else if let Some(rest) = prefix.strip_prefix("/home/") {
            let name = rest.split('/').next().unwrap_or("");
            if !valid_user(name) {
                continue;
            }
            name.to_string()
        } else {
            continue;
        };
        let home = format!("{prefix}/.pm2");
        if valid_home(&home) {
            return Some((user, home));
        }
    }
    None
}

pub fn probe_is_up(kind: &str, target: &str, output: &str) -> bool {
    match kind {
        "pm2" => pm2_is_online(output, target),
        "systemd" => output.trim() == "active",
        "docker" => output.trim() == "true",
        "nginx" | "apache" | "process" => output.trim() == "up",
        _ => false,
    }
}

pub fn health_reached(verb: &str, up: bool) -> bool {
    if verb == "stop" {
        !up
    } else {
        up
    }
}

pub fn health_message(verb: &str, reached: bool, waited_secs: u64, timeout_secs: u64) -> String {
    if reached {
        format!("Healthy after {waited_secs}s.")
    } else if verb == "stop" {
        format!("Still running after {timeout_secs}s.")
    } else {
        format!("Not back online within {timeout_secs}s.")
    }
}

pub fn pm2_is_online(json: &str, name: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return false;
    };
    let Some(list) = value.as_array() else {
        return false;
    };
    list.iter().any(|row| {
        row.get("name").and_then(|v| v.as_str()) == Some(name)
            && row
                .get("pm2_env")
                .and_then(|env| env.get("status"))
                .and_then(|v| v.as_str())
                == Some("online")
    })
}

pub fn plan_action(input: &PlanInput, require_confirm: bool) -> Result<PlannedAction, String> {
    let kind = input.kind.trim();
    let verb = input.verb.trim();
    let target = input.target.trim();
    if !valid_token(target) {
        return Err(
            "Name can only contain letters, numbers, and . _ : @ - / (no spaces or ..)."
                .into(),
        );
    }

    let (pm2_user, pm2_home) = resolve_pm2(input, kind)?;
    let mut use_sudo = input.use_sudo;
    if kind == "pm2" {
        if let Some(user) = pm2_user.as_deref() {
            if user != input.ssh_user.trim() {
                use_sudo = true;
            }
        }
    }

    let dangerous = verb == "stop";
    if require_confirm && dangerous {
        let typed = input.confirm_name.as_deref().map(str::trim).unwrap_or("");
        if typed != target {
            return Err(format!("Type the name \"{target}\" to confirm stop."));
        }
    }

    let sudo_user = pm2_user
        .as_deref()
        .filter(|user| kind == "pm2" && *user != input.ssh_user.trim());
    let steps = match kind {
        "pm2" => vec![PlannedStep {
            command: apply_sudo(
                &pm2_shell(verb, target, pm2_home.as_deref())?,
                use_sudo,
                input.sudo_password_set,
                sudo_user,
            ),
            skip_if_prev_failed: false,
        }],
        "systemd" => {
            let systemctl_verb = match verb {
                "start" | "stop" | "restart" | "reload" => verb,
                other => return Err(format!("Unsupported systemd action: {other}")),
            };
            vec![PlannedStep {
                command: apply_sudo(
                    &format!("systemctl {systemctl_verb} -- {}", shell_quote(target)),
                    use_sudo,
                    input.sudo_password_set,
                    None,
                ),
                skip_if_prev_failed: false,
            }]
        }
        "docker" => {
            let docker_verb = match verb {
                "start" | "stop" | "restart" => verb,
                other => return Err(format!("Unsupported docker action: {other}")),
            };
            vec![PlannedStep {
                command: apply_sudo(
                    &format!("docker {docker_verb} -- {}", shell_quote(target)),
                    use_sudo,
                    input.sudo_password_set,
                    None,
                ),
                skip_if_prev_failed: false,
            }]
        }
        "nginx" => {
            if verb != "reload" {
                return Err("Nginx only supports a config test followed by reload.".into());
            }
            nginx_or_apache_steps(use_sudo, input.sudo_password_set, "nginx -t", "nginx -s reload")
        }
        "apache" => {
            if verb != "reload" {
                return Err("Apache only supports a config test followed by reload.".into());
            }
            nginx_or_apache_steps(
                use_sudo,
                input.sudo_password_set,
                "if command -v apache2ctl >/dev/null 2>&1; then apache2ctl configtest; elif command -v httpd >/dev/null 2>&1; then httpd -t; else apachectl configtest; fi",
                "if command -v apache2ctl >/dev/null 2>&1; then apache2ctl graceful; elif command -v httpd >/dev/null 2>&1; then httpd -k graceful; else apachectl graceful; fi",
            )
        }
        "process" => {
            if verb != "stop" {
                return Err("A process only supports SIGTERM.".into());
            }
            if !target.chars().all(|c| c.is_ascii_digit()) {
                return Err("PID must be a number.".into());
            }
            vec![PlannedStep {
                command: apply_sudo(
                    &format!("kill -TERM -- {}", shell_quote(target)),
                    use_sudo,
                    input.sudo_password_set,
                    None,
                ),
                skip_if_prev_failed: false,
            }]
        }
        _ => return Err("This entry has no guarded action.".into()),
    };

    let health_command = health_shell(
        kind,
        verb,
        target,
        pm2_home.as_deref(),
        use_sudo,
        input.sudo_password_set,
        sudo_user,
    )?;

    Ok(PlannedAction {
        steps,
        health_command,
        dangerous,
        confirm_name: target.to_string(),
        target: target.to_string(),
        kind: kind.to_string(),
        verb: verb.to_string(),
        pm2_user,
        pm2_home,
        use_sudo,
        health_timeout_secs: clamp_health_timeout(input.health_timeout_secs),
    })
}

fn resolve_pm2(input: &PlanInput, kind: &str) -> Result<(Option<String>, Option<String>), String> {
    if kind != "pm2" {
        return Ok((None, None));
    }
    let explicit_user = non_empty(input.pm2_user.as_deref());
    let explicit_home = non_empty(input.pm2_home.as_deref());
    let (user, home) = match (explicit_user, explicit_home) {
        (None, None) => infer_pm2(&input.log_paths)
            .map(|(u, h)| (Some(u), Some(h)))
            .unwrap_or((None, None)),
        (user, home) => (user.map(str::to_string), home.map(str::to_string)),
    };
    if let Some(user) = user.as_deref() {
        if !valid_user(user) {
            return Err("PM2 user must be a Linux account name.".into());
        }
    }
    if let Some(home) = home.as_deref() {
        if !valid_home(home) {
            return Err("PM2_HOME must be an absolute path without ..".into());
        }
    }
    Ok((user, home))
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn pm2_shell(verb: &str, target: &str, home: Option<&str>) -> Result<String, String> {
    let verb = match verb {
        "start" | "stop" | "restart" | "reload" => verb,
        other => return Err(format!("Unsupported PM2 action: {other}")),
    };
    let cmd = format!("pm2 {verb} -- {}", shell_quote(target));
    Ok(match home {
        Some(home) => format!("env PM2_HOME={} {cmd}", shell_quote(home)),
        None => cmd,
    })
}

fn nginx_or_apache_steps(
    use_sudo: bool,
    password_set: bool,
    test_cmd: &str,
    reload_cmd: &str,
) -> Vec<PlannedStep> {
    vec![
        PlannedStep {
            command: apply_sudo(test_cmd, use_sudo, password_set, None),
            skip_if_prev_failed: false,
        },
        PlannedStep {
            command: apply_sudo(reload_cmd, use_sudo, password_set, None),
            skip_if_prev_failed: true,
        },
    ]
}

fn health_shell(
    kind: &str,
    verb: &str,
    target: &str,
    pm2_home: Option<&str>,
    use_sudo: bool,
    password_set: bool,
    sudo_user: Option<&str>,
) -> Result<String, String> {
    let _ = verb;
    let inner = match kind {
        "pm2" => match pm2_home {
            Some(home) => format!("env PM2_HOME={} pm2 jlist", shell_quote(home)),
            None => "pm2 jlist".to_string(),
        },
        "systemd" => format!("systemctl is-active -- {}", shell_quote(target)),
        "docker" => format!(
            "docker inspect -f '{{{{.State.Running}}}}' {}",
            shell_quote(target)
        ),
        "nginx" => "if pidof nginx >/dev/null 2>&1; then echo up; else echo down; fi".to_string(),
        "apache" => {
            "if pidof apache2 >/dev/null 2>&1 || pidof httpd >/dev/null 2>&1; then echo up; else echo down; fi"
                .to_string()
        }
        "process" => format!(
            "if ps -p {} >/dev/null 2>&1; then echo up; else echo down; fi",
            shell_quote(target)
        ),
        _ => return Err("This entry has no guarded action.".into()),
    };
    let sudo = match kind {
        "nginx" | "apache" | "systemd" => false,
        _ => use_sudo,
    };
    Ok(apply_sudo(&inner, sudo, password_set, sudo_user))
}

fn apply_sudo(inner: &str, use_sudo: bool, password_set: bool, user: Option<&str>) -> String {
    if !use_sudo && user.is_none() {
        return inner.to_string();
    }
    let mut command = if password_set {
        "sudo -S -p ''".to_string()
    } else {
        "sudo -n".to_string()
    };
    if let Some(user) = user {
        command.push_str(" -u ");
        command.push_str(&shell_quote(user));
    }
    command.push_str(" -- ");
    command.push_str(inner);
    command
}

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.contains("..")
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '@' | '-' | '/'))
}

fn valid_user(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        _ => return false,
    }
    value.len() <= 32
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | '.'))
}

fn valid_home(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 256
        && !value.contains("..")
        && !value.contains('\0')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '.' | '-' ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> PlanInput {
        PlanInput {
            kind: "pm2".into(),
            verb: "restart".into(),
            target: "api".into(),
            ssh_user: "ubuntu".into(),
            pm2_user: None,
            pm2_home: None,
            log_paths: vec![],
            use_sudo: false,
            sudo_password_set: false,
            confirm_name: None,
            health_timeout_secs: None,
        }
    }

    #[test]
    fn process_stop_sends_sigterm() {
        let mut input = base();
        input.kind = "process".into();
        input.verb = "stop".into();
        input.target = "4242".into();
        input.use_sudo = false;
        let plan = plan_action(&input, false).unwrap();
        assert!(plan.display().contains("kill -TERM"));
        assert!(plan.display().contains("4242"));
        assert!(plan.dangerous);
        assert!(plan.health_command.contains("ps -p"));
    }

    #[test]
    fn quotes_embedded_single_quote() {
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn pm2_same_user_with_home_has_no_sudo() {
        let mut input = base();
        input.pm2_user = Some("ubuntu".into());
        input.pm2_home = Some("/home/ubuntu/.pm2".into());
        let plan = plan_action(&input, false).unwrap();
        assert_eq!(
            plan.display(),
            "env PM2_HOME='/home/ubuntu/.pm2' pm2 restart -- 'api'"
        );
        assert!(!plan.use_sudo);
        assert!(plan.health_command.contains("pm2 jlist"));
    }

    #[test]
    fn pm2_other_user_forces_sudo_and_home() {
        let mut input = base();
        input.pm2_user = Some("deploy".into());
        input.pm2_home = Some("/home/deploy/.pm2".into());
        input.sudo_password_set = true;
        let plan = plan_action(&input, false).unwrap();
        assert_eq!(
            plan.display(),
            "sudo -S -p '' -u 'deploy' -- env PM2_HOME='/home/deploy/.pm2' pm2 restart -- 'api'"
        );
        assert!(plan.use_sudo);
        assert!(needs_sudo_stdin(&plan.display()));
    }

    #[test]
    fn infers_pm2_from_log_path() {
        let found = infer_pm2(&["/home/deploy/.pm2/logs/api-out.log".into()]).unwrap();
        assert_eq!(found.0, "deploy");
        assert_eq!(found.1, "/home/deploy/.pm2");
        let root = infer_pm2(&["/root/.pm2/logs/api-error.log".into()]).unwrap();
        assert_eq!(root.0, "root");
        assert_eq!(root.1, "/root/.pm2");
        assert!(infer_pm2(&["/var/log/app.log".into()]).is_none());
    }

    #[test]
    fn stop_requires_typed_name() {
        let mut input = base();
        input.verb = "stop".into();
        let err = plan_action(&input, true).unwrap_err();
        assert!(err.contains("api"));
        input.confirm_name = Some("nope".into());
        assert!(plan_action(&input, true).is_err());
        input.confirm_name = Some("api".into());
        let plan = plan_action(&input, true).unwrap();
        assert!(plan.dangerous);
        assert!(plan.display().contains("pm2 stop -- 'api'"));
    }

    #[test]
    fn preview_does_not_require_the_typed_name() {
        let mut input = base();
        input.verb = "stop".into();
        let plan = plan_action(&input, false).unwrap();
        assert!(plan.dangerous);
        assert_eq!(plan.confirm_name, "api");
    }

    #[test]
    fn systemd_and_docker_commands() {
        let mut input = base();
        input.kind = "systemd".into();
        input.verb = "reload".into();
        input.target = "nginx".into();
        input.use_sudo = true;
        let plan = plan_action(&input, false).unwrap();
        assert_eq!(plan.display(), "sudo -n -- systemctl reload -- 'nginx'");
        assert_eq!(plan.health_command, "systemctl is-active -- 'nginx'");

        input.kind = "docker".into();
        input.verb = "stop".into();
        input.target = "web".into();
        input.confirm_name = Some("web".into());
        input.sudo_password_set = false;
        let plan = plan_action(&input, true).unwrap();
        assert_eq!(plan.display(), "sudo -n -- docker stop -- 'web'");
        assert!(plan.health_command.contains("{{.State.Running}}"));
        assert!(plan.health_command.contains("'web'"));
    }

    #[test]
    fn nginx_reload_is_gated_on_the_test() {
        let mut input = base();
        input.kind = "nginx".into();
        input.verb = "reload".into();
        input.target = "nginx".into();
        input.use_sudo = true;
        let plan = plan_action(&input, false).unwrap();
        assert_eq!(plan.steps.len(), 2);
        assert!(plan.steps[0].command.contains("nginx -t"));
        assert!(!plan.steps[0].skip_if_prev_failed);
        assert!(plan.steps[1].command.contains("nginx -s reload"));
        assert!(plan.steps[1].skip_if_prev_failed);
        assert!(!step_should_run(true, true));
        assert!(step_should_run(false, true));
        input.verb = "stop".into();
        assert!(plan_action(&input, false).unwrap_err().contains("Nginx"));
    }

    #[test]
    fn apache_configtest_gates_graceful() {
        let mut input = base();
        input.kind = "apache".into();
        input.verb = "reload".into();
        input.target = "apache:example.com".into();
        input.use_sudo = false;
        let plan = plan_action(&input, false).unwrap();
        assert!(plan.steps[0].command.contains("configtest"));
        assert!(plan.steps[1].command.contains("graceful"));
        assert!(plan.steps[1].skip_if_prev_failed);
        assert_eq!(plan.health_command.contains("pidof"), true);
    }

    #[test]
    fn rejects_metacharacters() {
        let mut input = base();
        input.target = "api;rm".into();
        assert!(plan_action(&input, false).is_err());
        input.target = "../api".into();
        assert!(plan_action(&input, false).is_err());
        input.kind = "service".into();
        input.target = "mysql".into();
        assert!(plan_action(&input, false).unwrap_err().contains("no guarded"));
    }

    #[test]
    fn health_interpretation() {
        let json = r#"[{"name":"api","pm2_env":{"status":"online"}},{"name":"web","pm2_env":{"status":"stopped"}}]"#;
        assert!(pm2_is_online(json, "api"));
        assert!(!pm2_is_online(json, "web"));
        assert!(!pm2_is_online("nope", "api"));
        assert!(probe_is_up("systemd", "nginx", "active\n"));
        assert!(!probe_is_up("systemd", "nginx", "failed\n"));
        assert!(probe_is_up("docker", "web", "true\n"));
        assert!(probe_is_up("nginx", "nginx", "up\n"));
        assert!(!probe_is_up("apache", "apache", "down"));
        assert!(health_reached("stop", false));
        assert!(!health_reached("stop", true));
        assert!(health_reached("restart", true));
        assert!(!health_reached("reload", false));
        assert_eq!(
            health_message("restart", false, 15, 15),
            "Not back online within 15s."
        );
        assert_eq!(
            health_message("stop", false, 15, 15),
            "Still running after 15s."
        );
        assert_eq!(clamp_health_timeout(None), 15);
        assert_eq!(clamp_health_timeout(Some(0)), 1);
        assert_eq!(clamp_health_timeout(Some(99)), 60);
    }

    #[test]
    fn excerpt_and_redaction() {
        assert_eq!(excerpt("ok"), "ok");
        let long = "a".repeat(500);
        assert_eq!(excerpt(&long).chars().count(), 500);
        assert_eq!(excerpt(&format!("{long}b")).chars().count(), 501);
        assert!(excerpt(&format!("{long}b")).ends_with('…'));
        assert_eq!(redact("secret-value failed", Some("secret-value")), "•••• failed");
        assert_eq!(redact("ab in output", Some("ab")), "ab in output");
        assert!(!actor_name().is_empty());
    }
}
