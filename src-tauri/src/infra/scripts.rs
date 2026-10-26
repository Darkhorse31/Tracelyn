//! Only read-only commands. Dynamic values are single-quoted, never interpreted as shell code.
use super::model::{Credentials, Target};
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn database(engine: &str, target: &Target, credentials: &Credentials) -> String {
    database_command(engine, target, credentials, false)
}
pub fn mysql_modern(target: &Target, credentials: &Credentials) -> String {
    database_command("mysql", target, credentials, true)
}
fn database_command(
    engine: &str,
    target: &Target,
    credentials: &Credentials,
    modern_replica: bool,
) -> String {
    let host = if target.host.is_empty() {
        "localhost"
    } else {
        &target.host
    };
    let user = if credentials.username.is_empty() {
        "".into()
    } else {
        format!(" --username {}", quote(&credentials.username))
    };
    let (cli, image, environment, args) = match engine {
        "mysql" => {
            let user = if credentials.username.is_empty() {
                "".into()
            } else {
                format!(" --user={}", quote(&credentials.username))
            };
            let sql="SHOW GLOBAL STATUS WHERE Variable_name IN ('Threads_connected','Queries','Slow_queries','Uptime','Innodb_buffer_pool_reads','Innodb_buffer_pool_read_requests'); SELECT '__SIZES__'; SELECT table_schema,COALESCE(SUM(data_length+index_length),0) FROM information_schema.tables GROUP BY table_schema; SELECT '__REPLICATION__'; SHOW SLAVE STATUS\\G";
            let sql = if modern_replica {
                sql.replace("SHOW SLAVE STATUS", "SHOW REPLICA STATUS")
            } else {
                sql.into()
            };
            ("mysql","mysql|mariadb",vec![("MYSQL_PWD",credentials.password.clone())],format!(" --batch --skip-column-names --connect-timeout=4 --host={} --port={} {user} --execute {}",quote(host),target.port.unwrap_or(3306),quote(&sql)))
        }
        "postgres" => {
            let host_option = if target.host.is_empty() {
                String::new()
            } else {
                format!(" --host={}", quote(host))
            };
            let sql = r#"SELECT json_build_object('connections',(SELECT count(*) FROM pg_stat_activity),'queryCounter',sum(xact_commit+xact_rollback),'counterEpoch',max(stats_reset)::text,'slowQueries',(SELECT count(*) FROM pg_stat_activity WHERE state='active' AND pid<>pg_backend_pid() AND now()-query_start>interval '1 second'),'cacheHitPercent',100.0*sum(blks_hit)/NULLIF(sum(blks_hit+blks_read),0),'replicationLagSeconds',CASE WHEN pg_is_in_recovery() THEN CASE WHEN pg_last_wal_receive_lsn()=pg_last_wal_replay_lsn() THEN 0 ELSE EXTRACT(EPOCH FROM now()-pg_last_xact_replay_timestamp()) END ELSE (SELECT EXTRACT(EPOCH FROM max(replay_lag)) FROM pg_stat_replication) END,'sizes',(SELECT json_agg(json_build_object('database',datname,'bytes',pg_database_size(oid))) FROM pg_database WHERE datallowconn)) FROM pg_stat_database"#;
            let db = if target.database.is_empty() {
                "postgres"
            } else {
                &target.database
            };
            (
                "psql",
                "postgres",
                vec![
                    ("PGPASSWORD", credentials.password.clone()),
                    (
                        "PGOPTIONS",
                        "-c default_transaction_read_only=on -c statement_timeout=5000".into(),
                    ),
                    ("PGCONNECT_TIMEOUT", "4".into()),
                ],
                format!(
                    " --no-password -X -A -t {host_option} --port={} {user} --dbname={} --command {}",
                    target.port.unwrap_or(5432),
                    quote(db),
                    quote(sql)
                ),
            )
        }
        _ => {
            let db = if target.database.is_empty() {
                "admin"
            } else {
                &target.database
            };
            let db_json = serde_json::to_string(db).unwrap_or_else(|_| "\"admin\"".into());
            let script = format!(
                r#"const a=db.getSiblingDB('admin'); const out={{status:a.serverStatus(),warnings:[]}}; try{{out.sizes=a.runCommand({{listDatabases:1}}).databases.map(x=>({{database:x.name,bytes:Number(x.sizeOnDisk)}}));}}catch(e){{out.warnings.push('listDatabases permission required')}} try{{out.replication=a.runCommand({{replSetGetStatus:1}});if(out.replication.members)out.replication.members=out.replication.members.map(x=>({{self:x.self,optimeDateMs:x.optimeDate?x.optimeDate.getTime():null}}));}}catch(e){{out.warnings.push('Replication status unavailable')}} const d=db.getSiblingDB({db_json});try{{if(d.getCollectionNames().includes('system.profile'))out.slowQueries=d.system.profile.countDocuments({{millis:{{$gte:100}}}},{{maxTimeMS:3000}});}}catch(e){{out.warnings.push('system.profile unavailable')}} print(JSON.stringify(out));"#
            );
            // Environment avoids placing a password in mongosh's command-line arguments.
            let auth = if credentials.username.is_empty() {
                String::new()
            } else {
                format!(
                    " --username {} --authenticationDatabase admin",
                    quote(&credentials.username)
                )
            };
            let password_js = if credentials.password.is_empty() {
                String::new()
            } else {
                "const auth=db.getSiblingDB('admin').auth(process.env.TL_MONGO_USER,process.env.TL_MONGO_PASSWORD); if(!auth)throw Error('Authentication failed'); ".into()
            };
            let auth = if credentials.password.is_empty() {
                auth
            } else {
                String::new()
            };
            (
                "mongosh",
                "mongo",
                vec![
                    ("TL_MONGO_USER", credentials.username.clone()),
                    ("TL_MONGO_PASSWORD", credentials.password.clone()),
                ],
                format!(
                    " --quiet --host={} --port={} {auth} --eval {}",
                    quote(host),
                    target.port.unwrap_or(27017),
                    quote(&(password_js + &script))
                ),
            )
        }
    };
    let env = environment
        .iter()
        .map(|(key, val)| format!("{key}={}", quote(val)))
        .collect::<Vec<_>>()
        .join(" ");
    let docker_env = environment
        .iter()
        .map(|(key, val)| format!(" -e {key}={}", quote(val)))
        .collect::<String>();
    if !target.container.is_empty() {
        return format!(
            "docker exec{docker_env} {} {cli}{args}",
            quote(&target.container)
        );
    }
    format!("if command -v {cli} >/dev/null 2>&1; then env {env} {cli}{args}; elif command -v docker >/dev/null 2>&1; then c=$(docker ps --format '{{{{.Names}}}} {{{{.Image}}}}' | awk 'tolower($2) ~ /{image}/ {{print $1; exit}}'); if [ -n \"$c\" ]; then docker exec{docker_env} \"$c\" {cli}{args}; else echo 'Client and matching database container not installed' >&2; exit 127; fi; else echo 'Database CLI not installed' >&2; exit 127; fi")
}

pub fn rabbitmq(credentials: &Credentials, target: &Target) -> String {
    let port = target.port.unwrap_or(15672);
    let auth = if credentials.username.is_empty() {
        String::new()
    } else {
        format!(
            " --user {}",
            quote(&format!(
                "{}:{}",
                credentials.username, credentials.password
            ))
        )
    };
    format!(
        r#"if result=$(curl --silent --show-error --fail --max-time 4{auth} 'http://127.0.0.1:{port}/api/queues?pagination=true&page=1&page_size=500' 2>/dev/null); then printf '%s' "$result"; elif command -v rabbitmqctl >/dev/null 2>&1; then rabbitmqctl -s list_vhosts name | head -n 32 | while IFS= read -r vhost; do printf '__VHOST__\t%s\n' "$vhost"; rabbitmqctl -q list_queues -p "$vhost" name messages messages_ready messages_unacknowledged consumers state --formatter json; done; else curl --silent --show-error --fail --max-time 4{auth} 'http://127.0.0.1:{port}/api/queues?pagination=true&page=1&page_size=500'; fi"#
    )
}
pub const KAFKA: &str = r#"topics=$(command -v kafka-topics || command -v kafka-topics.sh || printf '%s' /opt/kafka/bin/kafka-topics.sh); groups=$(command -v kafka-consumer-groups || command -v kafka-consumer-groups.sh || printf '%s' /opt/kafka/bin/kafka-consumer-groups.sh); "$topics" --bootstrap-server localhost:9092 --describe && "$groups" --bootstrap-server localhost:9092 --all-groups --describe"#;
pub const HAPROXY: &str = r#"for socket in /run/haproxy/admin.sock /var/run/haproxy.sock /var/lib/haproxy/stats; do if [ -S "$socket" ]; then if command -v socat >/dev/null 2>&1; then printf 'show stat\n' | socat -T 4 - UNIX-CONNECT:"$socket"; exit $?; elif command -v nc >/dev/null 2>&1; then printf 'show stat\n' | nc -w 4 -U "$socket"; exit $?; fi; fi; done; curl --silent --show-error --fail --max-time 5 'http://127.0.0.1:8404/stats;csv'"#;
pub const NGINX: &str = "nginx -T 2>&1";
pub const NGINX_FILES: &str = r#"for f in /etc/nginx/nginx.conf /etc/nginx/sites-enabled/* /etc/nginx/conf.d/*; do if [ -f "$f" ] && [ -r "$f" ]; then printf '\n# %s\n' "$f"; cat -- "$f"; fi; done"#;
pub const PRESENCE: &str = r#"echo '---bins---'; for b in haproxy traefik caddy nginx apache2 httpd mysql mariadbd psql postgres mongod mongosh docker kubectl k3s fail2ban-client rabbitmqctl kafka-topics redis-server; do command -v "$b" >/dev/null 2>&1 && echo "$b"; done; echo '---units---'; for u in haproxy traefik caddy nginx apache2 mysql mariadb postgresql mongod docker redis-server rabbitmq-server fail2ban; do if systemctl list-unit-files "${u}.service" --no-legend --no-pager 2>/dev/null | grep -q "$u"; then st=$(systemctl is-active "$u" 2>/dev/null || true); echo "$u ${st:-unknown}"; fi; done; echo '---ss---'; ss -ltnpH 2>/dev/null || true; echo '---docker---'; docker ps --format '{{.Image}} {{.Names}}' 2>/dev/null || true"#;
pub const TRAEFIK: &str =
    "curl --silent --show-error --fail --max-time 5 http://127.0.0.1:8080/api/http/services";
pub const CADDY: &str =
    "curl --silent --show-error --fail --max-time 5 http://127.0.0.1:2019/reverse_proxy/upstreams";
pub const DOCKER: &str = r#"ids=$(docker ps -aq --no-trunc) || exit $?; ids=$(printf '%s\n' "$ids" | head -n 500); if [ -z "$ids" ]; then printf '[]\n__STATS__\n'; else docker inspect --format '{"Id":{{json .Id}},"Name":{{json .Name}},"RestartCount":{{json .RestartCount}},"State":{"Status":{{json .State.Status}},"Health":{{if .State.Health}}{"Status":{{json .State.Health.Status}}}{{else}}null{{end}}},"Config":{"Image":{{json .Config.Image}},"Labels":{{json .Config.Labels}}},"NetworkSettings":{"Ports":{{json .NetworkSettings.Ports}}}}' $ids | awk 'BEGIN{printf "["} NR>1{printf ","} {printf "%s",$0} END{print "]"}'; printf '\n__STATS__\n'; docker stats --no-stream --format '{{json .}}'; fi"#;
pub const KUBERNETES:&str="if command -v kubectl >/dev/null 2>&1; then kubectl --request-timeout=6s get nodes,pods --all-namespaces -o json; elif command -v k3s >/dev/null 2>&1; then k3s kubectl --request-timeout=6s get nodes,pods --all-namespaces -o json; else echo 'kubectl or k3s not installed' >&2; exit 127; fi";
pub const DOMAINS: &str = r#"nginx -T 2>&1; for f in /etc/apache2/sites-enabled/* /etc/httpd/conf/httpd.conf /etc/httpd/conf.d/*.conf; do [ ! -e "$f" ] || cat -- "$f"; done; true"#;
pub const CERTIFICATES: &str = r#"command -v openssl >/dev/null 2>&1 || exit 127; for f in /etc/letsencrypt/live/*/cert.pem /etc/letsencrypt/live/*/fullchain.pem; do if [ -e "$f" ]; then printf '__CERT__\t%s\n' "$f"; openssl x509 -in "$f" -noout -enddate -subject; fi; done"#;
pub const CAPACITY: &str = "LC_ALL=C df -Pk -x tmpfs -x devtmpfs -x squashfs";
pub const SSH_SECURITY: &str = r#"printf '__OFFSET__\t%s\n' "$(date +%z)"; if journalctl --no-pager -q -o short-iso --since '24 hours ago' -n 10000 -u ssh -u sshd 2>/dev/null | grep -E 'Failed password|Invalid user|authentication failure'; then :; elif [ -r /var/log/auth.log ]; then tail -n 10000 /var/log/auth.log; elif [ -r /var/log/secure ]; then tail -n 10000 /var/log/secure; else journalctl --no-pager -q -o short-iso --since '24 hours ago' -n 10000 -u ssh -u sshd; fi"#;
pub const PORTS: &str = "ss -lntupH";
pub const UPDATES: &str = r#"export LC_ALL=C; if command -v apt >/dev/null 2>&1; then apt list --upgradable 2>/dev/null; elif command -v dnf >/dev/null 2>&1; then dnf -q --cacheonly check-update; code=$?; [ "$code" = 0 ] || [ "$code" = 100 ] || exit "$code"; printf '\n__SECURITY__\n'; dnf -q --cacheonly check-update --security; code=$?; [ "$code" = 0 ] || [ "$code" = 100 ]; else echo 'apt or dnf not installed' >&2; exit 127; fi"#;
pub const FAIL2BAN: &str = r#"status=$(fail2ban-client status) || exit $?; jails=$(printf '%s' "$status" | sed -n 's/.*Jail list:[[:space:]]*//p' | tr ',' ' '); for jail in $jails; do printf '__JAIL__\t%s\n' "$jail"; fail2ban-client status "$jail"; done"#;
pub const LAST_LOGINS: &str = "last -F -i -n 100";
pub const CRON: &str = r#"tz=$(cat /etc/timezone 2>/dev/null || timedatectl show -p Timezone --value 2>/dev/null); printf '__TZ__\t%s\n' "${tz:-UTC}"; getent passwd | cut -d: -f1 | head -n 256 | while IFS= read -r user; do printf '__CRON__\t%s\tuser crontab\tuser\n' "$user"; crontab -u "$user" -l 2>&1; done; for f in /etc/crontab /etc/cron.d/*; do if [ -f "$f" ]; then printf '__CRON__\troot\t%s\tsystem\n' "$f"; cat -- "$f"; fi; done; printf '\n__JOURNAL__\n'; journalctl -q --no-pager -o short-iso -n 1000 --since '7 days ago' -u cron -u crond 2>/dev/null; [ ! -r /var/log/syslog ] || tail -n 1000 /var/log/syslog | grep CRON; printf '\n__TIMERS__\n'; systemctl list-units --type=timer --all --plain --no-legend --no-pager | head -n 256 | while read -r timer rest; do printf '__TIMER__\t%s\n' "$timer"; systemctl show "$timer" -p Unit -p NextElapseUSecRealtime -p LastTriggerUSec; unit=$(systemctl show "$timer" -p Unit --value); [ -z "$unit" ] || systemctl show "$unit" -p Result -p ExecMainStatus; done"#;
