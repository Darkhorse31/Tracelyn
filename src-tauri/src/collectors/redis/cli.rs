/// Builds `redis-cli` command strings for execution on the remote host via SSH.
#[derive(Debug, Clone)]
pub struct RedisCliArgs {
    subcommand: String,
}

impl RedisCliArgs {
    pub fn new(subcommand: impl Into<String>) -> Self {
        Self {
            subcommand: subcommand.into(),
        }
    }
}

pub fn build_redis_cli_invocation(
    redis_cli: &str,
    args: &RedisCliArgs,
    password: Option<&str>,
) -> String {
    let base = format!("{redis_cli} --no-auth-warning {}", args.subcommand);
    match password {
        None => base,
        Some(pw) => {
            let escaped = shell_single_quote(pw);
            format!("REDISCLI_AUTH={escaped} {base}")
        }
    }
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
        assert_eq!(cmd, "redis-cli --no-auth-warning INFO all");
    }

    #[test]
    fn builds_command_with_auth_env() {
        let cmd = build_redis_cli_invocation(
            "redis-cli",
            &RedisCliArgs::new("PING"),
            Some("s3cret"),
        );
        assert!(cmd.contains("REDISCLI_AUTH='s3cret'"));
        assert!(cmd.contains("PING"));
    }
}
