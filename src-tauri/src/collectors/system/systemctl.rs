use crate::metrics::ServiceRow;

pub const DEFAULT_SERVICE_CANDIDATES: &[&str] = &[
    "nginx",
    "apache2",
    "httpd",
    "redis",
    "redis-server",
    "mysql",
    "mariadb",
    "postgresql",
    "php-fpm",
    "php8.2-fpm",
    "php8.3-fpm",
];

pub fn parse_is_active(output: &str, service_names: &[&str]) -> Vec<ServiceRow> {
    let lines: Vec<&str> = output.lines().map(|l| l.trim()).collect();
    service_names
        .iter()
        .zip(lines.iter())
        .map(|(name, state)| {
            let state = state.to_string();
            let active = state == "active";
            ServiceRow {
                name: (*name).to_string(),
                active,
                state,
            }
        })
        .collect()
}

pub fn systemctl_is_active_command(services: &[&str]) -> String {
    format!(
        "systemctl is-active {} 2>/dev/null",
        services.join(" ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture() {
        let raw = std::fs::read_to_string("src/collectors/fixtures/systemctl.txt")
            .or_else(|_| {
                std::fs::read_to_string("src-tauri/src/collectors/fixtures/systemctl.txt")
            })
            .unwrap();
        let names = ["nginx", "apache2", "redis", "mysql"];
        let rows = parse_is_active(&raw, &names);
        assert_eq!(rows.len(), 4);
        assert!(rows[0].active);
        assert!(!rows[1].active);
        assert_eq!(rows[3].state, "failed");
    }
}
