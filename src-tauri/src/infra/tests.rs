use super::{cron, model::*, nginx, parsers, scripts};
#[cfg(windows)]
#[test]
fn windows_credentials_roundtrip_and_clear() {
    struct Cleanup(keyring::Entry);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.0.delete_credential();
        }
    }
    let server = format!("infra-fixture-{}", uuid::Uuid::new_v4());
    let secret = Cleanup(super::entry(&server, "mysql").unwrap());
    secret
        .0
        .set_password(
            &serde_json::to_string(&Credentials {
                username: "monitor_fixture".into(),
                password: "fixture-secret-never-sent".into(),
            })
            .unwrap(),
        )
        .unwrap();
    let roundtrip = super::credentials(&server, "mysql").unwrap();
    assert_eq!(roundtrip.username, "monitor_fixture");
    assert_eq!(roundtrip.password, "fixture-secret-never-sent");
    secret.0.delete_credential().unwrap();
    assert!(super::credentials(&server, "mysql")
        .unwrap()
        .password
        .is_empty());
}
const MYSQL: &str = include_str!("fixtures/mysql.txt");
#[test]
fn mysql_status_sizes_replica() {
    let rows = parsers::database("mysql", MYSQL).unwrap();
    let r = &rows[0];
    assert_eq!(r.number("connections"), Some(12.));
    assert_eq!(r.number("slowQueries"), Some(8.));
    assert_eq!(r.number("replicationLagSeconds"), Some(2.));
    assert_eq!(r.number("cacheHitPercent"), Some(99.));
    assert_eq!(r.fields["sizes"][0]["database"], "orders");
    assert!(parsers::database("mysql", "ERROR 1045 Access denied").is_err());
}
#[test]
fn postgres_and_mongo() {
    let p = parsers::database("postgres", include_str!("fixtures/postgres.json")).unwrap();
    assert_eq!(p[0].number("slowQueries"), Some(2.));
    assert!(p[0].number("replicationLagSeconds").is_none());
    let m = parsers::database("mongo", include_str!("fixtures/mongo.json")).unwrap();
    assert_eq!(m[0].number("queryCounter"), Some(150.));
    assert_eq!(m[0].number("cacheHitPercent"), Some(90.));
    assert_eq!(m[0].number("replicationLagSeconds"), Some(3.));
    assert!(parsers::database("mongo", "not json").is_err());
}
#[test]
fn rabbitmq_api_and_cli() {
    let api = parsers::rabbitmq(include_str!("fixtures/rabbitmq.json")).unwrap();
    assert_eq!(api[0].number("depth"), Some(42.));
    assert_eq!(api[0].number("publishPerSecond"), Some(3.5));
    let cli = parsers::rabbitmq("[{\"name\":\"jobs\",\"messages\":0,\"consumers\":1}]").unwrap();
    assert!(cli[0].number("publishPerSecond").is_none());
    let multi=parsers::rabbitmq("__VHOST__\t/\n[{\"name\":\"jobs\",\"messages\":2}]\n__VHOST__\t/shop\n[{\"name\":\"orders\",\"messages\":3}]\n").unwrap();
    assert_eq!(multi.len(), 2);
    assert_eq!(multi[1].name, "/shop / orders");
}
#[test]
fn kafka_partition_offsets_and_missing_offsets() {
    let rows = parsers::kafka(include_str!("fixtures/kafka.txt"));
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1].number("lag"), Some(20.));
    assert!(rows[2].number("lag").is_none());
    assert_eq!(rows[0].number("partitions"), Some(2.));
}
#[test]
fn haproxy_columns_not_positions_and_quoted_csv() {
    let rows = parsers::haproxy(include_str!("fixtures/haproxy.csv"));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "api / one, blue");
    assert_eq!(rows[0].state, "up");
    assert_eq!(rows[0].number("errorPercent"), Some(2.));
    assert_eq!(rows[1].state, "down");
}
#[test]
fn nginx_upstreams_weight_disabled_direct_and_logs() {
    let config = include_str!("fixtures/nginx.conf");
    let mut rows = nginx::upstreams(config);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].number("weight"), Some(3.));
    assert_eq!(rows[1].state, "down");
    assert_eq!(nginx::log_paths(config).len(), 2);
    nginx::add_log_health(&mut rows, include_str!("fixtures/nginx.log"));
    assert_eq!(rows[0].number("errorPercent"), Some(50.));
    assert_eq!(rows[0].number("recentConnectionErrors"), Some(1.));
}
#[test]
fn traefik_and_caddy_never_invent_rates() {
    let rows = parsers::proxy_api("traefik", include_str!("fixtures/traefik.json")).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].state, "up");
    assert!(rows[0].number("errorPercent").is_none());
    let rows = parsers::proxy_api("caddy", include_str!("fixtures/caddy.json")).unwrap();
    assert_eq!(rows[0].number("activeRequests"), Some(2.));
    assert_eq!(rows[0].state, "unknown");
}
#[test]
fn docker_inspect_stats_health_and_ports() {
    let rows = parsers::docker(include_str!("fixtures/docker.txt")).unwrap();
    assert_eq!(rows[0].name, "orders");
    assert_eq!(rows[0].number("cpuPercent"), Some(12.5));
    assert_eq!(rows[0].fields["health"], "healthy");
    assert_eq!(rows[0].fields["composeProject"], "shop");
    assert!(rows[0].fields["ports"]
        .as_str()
        .unwrap()
        .contains("0.0.0.0:8080"));
}
#[test]
fn k8s_nodes_pods_pending_restarts() {
    let rows = parsers::kubernetes(include_str!("fixtures/kubernetes.json")).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].state, "ready");
    assert_eq!(rows[1].number("restarts"), Some(4.));
    assert_eq!(rows[2].state, "Pending");
}
#[test]
fn domains_dedupe_and_certificate_thresholds() {
    assert_eq!(
        parsers::domains(include_str!("fixtures/domains.conf")),
        vec!["api.example.com", "example.com", "www.example.com"]
    );
    for invalid in [
        "*.example.com",
        "~.*",
        "$hostname",
        "_",
        "127.0.0.1",
        "example.com;rm",
        "-bad.example.com",
    ] {
        assert!(parsers::valid_domain(invalid).is_none());
    }
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-26T00:00:00Z")
        .unwrap()
        .timestamp_millis();
    let rows = parsers::certificates(include_str!("fixtures/certificates.txt"), now);
    assert_eq!(rows[0].state, "critical");
    assert_eq!(rows[1].state, "warning");
    assert_eq!(rows[2].state, "ok");
    let mut row = Resource::new("edge", "unknown");
    parsers::certificate_state(&mut row, now + 7 * 86_400_000, now);
    assert_eq!(row.state, "warning");
    parsers::certificate_state(&mut row, now + 21 * 86_400_000, now);
    assert_eq!(row.state, "ok");
}
#[test]
fn cron_users_etc_timezone_and_timer_exit() {
    let rows = cron::parse(include_str!("fixtures/cron.txt"), 1792972800000);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].fields["user"], "deploy");
    assert_eq!(rows[1].fields["user"], "root");
    assert_eq!(rows[2].fields["nextRun"], "At next reboot");
    let rows = cron::timers(include_str!("fixtures/timers.txt"));
    assert_eq!(rows[0].state, "warning");
    assert_eq!(rows[0].fields["exitCode"], "1");
}
#[test]
fn disk_all_mounts_ports_ipv6_and_process() {
    let rows = parsers::disks(include_str!("fixtures/df.txt"));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].name, "/data volume");
    assert_eq!(rows[0].number("usedBytes"), Some(52428800.));
    let rows = parsers::ports(include_str!("fixtures/ports.txt"));
    assert_eq!(rows[0].state, "exposed");
    assert_eq!(rows[1].state, "local");
    assert_eq!(rows[2].state, "exposed");
    assert_eq!(rows[0].name, "tcp sshd:22");
    let dup = parsers::ports(
        "tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=42,fd=3))\n\
         tcp LISTEN 0 128 [::]:22 [::]:* users:((\"sshd\",pid=42,fd=4))\n\
         tcp LISTEN 0 128 127.0.0.1:5432 0.0.0.0:* users:((\"postgres\",pid=52,fd=5))\n\
         tcp LISTEN 0 128 [::1]:5432 [::]:* users:((\"postgres\",pid=52,fd=6))\n",
    );
    assert_eq!(dup.len(), 2);
    assert_eq!(dup[0].name, "tcp sshd:22");
    assert_eq!(dup[0].fields["address"], "0.0.0.0:22, [::]:22");
    assert_eq!(dup[1].name, "tcp postgres:5432");
    assert!(dup[1].fields["address"].as_str().unwrap().contains("[::1]:5432"));
}
#[test]
fn security_logs_updates_bans_sessions() {
    let rows = parsers::ssh_failures(include_str!("fixtures/auth.log"));
    assert_eq!(rows[0].name, "198.51.100.4");
    assert_eq!(rows[0].number("attempts"), Some(2.));
    assert_eq!(rows.len(), 2);
    let rows = parsers::updates(include_str!("fixtures/updates.txt"));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().filter(|r| r.state == "security").count(), 1);
    let rows = parsers::fail2ban(include_str!("fixtures/fail2ban.txt"));
    assert_eq!(rows[0].number("banned"), Some(2.));
    assert!(rows[0].fields["bannedIps"]
        .as_str()
        .unwrap()
        .contains("198.51.100.4"));
    assert_eq!(
        parsers::last_logins(include_str!("fixtures/last.txt")).len(),
        1
    );
}
#[test]
fn shell_values_are_quoted() {
    assert_eq!(
        scripts::quote("x'$(touch /tmp/no)"),
        "'x'\\''$(touch /tmp/no)'"
    );
    let cmd = scripts::database(
        "mysql",
        &Target::default(),
        &Credentials {
            username: "observer".into(),
            password: "a' b;$(bad)".into(),
        },
    );
    assert!(cmd.contains("MYSQL_PWD='a'\\'' b;$(bad)'"));
    assert!(cmd.contains("SHOW SLAVE STATUS\\G"));
    let modern = scripts::mysql_modern(
        &Target::default(),
        &Credentials {
            username: "observer".into(),
            password: "SHOW SLAVE STATUS".into(),
        },
    );
    assert!(modern.contains("MYSQL_PWD='SHOW SLAVE STATUS'"));
    assert!(modern.contains("SHOW REPLICA STATUS\\G"));
    assert!(
        scripts::rabbitmq(&Credentials::default(), &Target::default())
            .contains("rabbitmqctl -s list_vhosts")
    );
}
#[test]
fn sqlite_check_retention_target_and_disk_roundtrip() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    super::store::migrate(&db).unwrap();
    super::store::migrate(&db).unwrap();
    super::store::save_target(
        &db,
        "s",
        "mysql",
        &Target {
            host: "localhost".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(super::store::target(&db, "s", "mysql").host, "localhost");
    super::store::save_domains(&db, "s", &["example.com".into(), "example.com".into()]).unwrap();
    assert_eq!(super::store::domains(&db, "s").unwrap().len(), 1);
    let check = UptimeCheck {
        id: "c".into(),
        server_id: "s".into(),
        url: "https://example.com/".into(),
        interval_seconds: 60,
        enabled: true,
    };
    db.execute(
        "INSERT INTO infra_checks VALUES('c','s',?1)",
        [serde_json::to_string(&check).unwrap()],
    )
    .unwrap();
    let now = crate::metrics_ingest::now_ms();
    let r = CheckResult {
        ts_ms: now,
        ok: true,
        status_code: Some(200),
        addresses: vec![],
        dns_ms: Some(2.),
        connect_ms: None,
        tls_ms: None,
        ttfb_ms: None,
        total_ms: 10.,
        error: None,
    };
    super::store::save_result(&db, "c", &r).unwrap();
    assert_eq!(
        super::store::check_views(&db, "s").unwrap()[0].history[0].status_code,
        Some(200)
    );
    let rows = parsers::disks(include_str!("fixtures/df.txt"));
    super::store::save_disk(&db, "s", &rows, now).unwrap();
    assert_eq!(super::store::disk_history(&db, "s", "/").unwrap().len(), 1);
}
