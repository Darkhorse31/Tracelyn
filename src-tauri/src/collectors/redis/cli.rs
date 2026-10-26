/// Builds `redis-cli` command strings for execution on the remote host via SSH.
#[derive(Debug, Clone)]
pub struct RedisCliArgs {
    subcommand: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub socket: Option<String>,
    pub username: Option<String>,
    pub docker_container: Option<String>,
    pub tls: bool,
}

impl RedisCliArgs {
    pub fn new(subcommand: impl Into<String>) -> Self {
        Self {
            subcommand: subcommand.into(),
            host: None,
            port: None,
            socket: None,
            username: None,
            docker_container: None,
            tls: false,
        }
    }
}

pub fn build_redis_cli_invocation(
    redis_cli: &str,
    args: &RedisCliArgs,
    password: Option<&str>,
) -> String {
    let mut cli = redis_cli.to_string();
    if args.tls {
        cli.push_str(" --tls");
    }
    if let Some(sock) = &args.socket {
        cli.push_str(" -s ");
        cli.push_str(&shell_single_quote(sock));
    } else {
        let host = args.host.as_deref().unwrap_or("127.0.0.1");
        cli.push_str(" -h ");
        cli.push_str(&shell_single_quote(host));
        if let Some(port) = args.port {
            cli.push_str(&format!(" -p {port}"));
        }
    }
    if let Some(user) = &args.username {
        cli.push_str(" --user ");
        cli.push_str(&shell_single_quote(user));
    }
    cli.push(' ');
    cli.push_str(&args.subcommand);

    let cmd = if let Some(container) = &args.docker_container {
        let mut exec = format!("docker exec {}", shell_single_quote(container));
        if let Some(pw) = password {
            exec.push_str(" -e REDISCLI_AUTH=");
            exec.push_str(&shell_single_quote(pw));
        }
        format!("{exec} {cli}")
    } else if let Some(pw) = password {
        format!("REDISCLI_AUTH={} {cli}", shell_single_quote(pw))
    } else {
        cli
    };

    format!("timeout 5 {cmd} </dev/null")
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_command_without_auth() {
        let cmd = build_redis_cli_invocation(
            "redis-cli",
            &RedisCliArgs::new("INFO all"),
            None,
        );
        assert!(cmd.contains("timeout 5 redis-cli -h '127.0.0.1' INFO all </dev/null"));
        assert!(!cmd.contains("REDISCLI_AUTH"));
        assert!(!cmd.contains("AUTH"));
    }

    #[test]
    fn builds_command_with_auth_env() {
        let mut args = RedisCliArgs::new("PING");
        args.port = Some(6380);
        args.username = Some("app".into());
        let cmd = build_redis_cli_invocation("redis-cli", &args, Some("s3cret"));
        assert!(cmd.contains("REDISCLI_AUTH='s3cret'"));
        assert!(cmd.contains("-h '127.0.0.1'"));
        assert!(cmd.contains("-p 6380"));
        assert!(cmd.contains("</dev/null"));
        assert!(cmd.contains("--user 'app'"));
        assert!(cmd.contains("PING"));
    }

    #[test]
    fn builds_docker_exec_with_socket() {
        let mut args = RedisCliArgs::new("INFO all");
        args.docker_container = Some("redis".into());
        args.socket = Some("/data/redis.sock".into());
        let cmd = build_redis_cli_invocation("redis-cli", &args, Some("pw"));
        assert!(cmd.contains("docker exec 'redis'"));
        assert!(cmd.contains("-e REDISCLI_AUTH='pw'"));
        assert!(cmd.contains("-s '/data/redis.sock'"));
        assert!(cmd.contains("timeout 5 "));
    }
}
