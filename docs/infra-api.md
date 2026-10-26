# Infrastructure API — Phase 3

All commands use camelCase arguments and JSON fields. Implementation lives in `src-tauri/src/infra/`, with React views in `src/pages/infra/` and components in `src/components/infra/`. No server software, files, settings, profiling, firewall rules, packages, jobs or services are changed.

## Tauri commands

| Command | Arguments | Result |
| --- | --- | --- |
| `get_infra_snapshot` | `{ serverId, collectors?: string[], includeHistory?: boolean, summaryOnly?: boolean }` | `Snapshot[]` |
| `refresh_infra` | `{ serverId }` | `void`; wakes each remote collector after its current pass |
| `get_infra_target` | `{ serverId, engine }` | `{ target: Target, hasCredentials: boolean }` |
| `set_infra_target` | `{ serverId, engine, target, credentials?: Credentials }` | `void`; saves and requests a refresh |
| `list_infra_domains` | `{ serverId }` | `string[]`, concrete discovered DNS names |
| `list_uptime_checks` | `{ serverId }` | `CheckView[]`, latest 240 results per check |
| `upsert_uptime_check` | `{ check: UptimeCheck }` | `UptimeCheck`; blank ID creates, existing ID edits |
| `delete_uptime_check` | `{ serverId, checkId }` | `void`; removes the check and its history |
| `run_uptime_check` | `{ serverId, checkId }` | `CheckResult`; executes a PC check now and stores it |

Every command validates that the server exists. Check ownership is validated for edits, deletes and manual execution. Supported credential engines: `mysql`, `postgres`, `mongo`, `rabbitmq`. Passwords and usernames are stored together in Windows Credential Manager as service `Tracelyn`, account `server:{serverId}:infra:{engine}`. They never appear in SQLite, diagnostics, snapshots or command responses. Omitting `credentials` preserves the entry; two empty strings remove it. Server deletion removes the four entries and Phase 3 SQLite data.

## Types

```ts
type Phase = "loading" | "ok" | "partial" | "not_installed"
  | "auth_needed" | "permission_denied" | "timed_out" | "error";

interface Resource {
  id: string;
  name: string;
  state: string; // source-specific state, e.g. up/down, running, Pending, exposed
  fields: Record<string, unknown>; // documented scalar/array fields below
}
interface Snapshot {
  collector: string;
  phase: Phase;
  message: string;
  fixHint: string | null;
  sampledAtMs: number;
  rows: Resource[];
}
interface Target {
  host: string; // blank: local CLI default; RabbitMQ API always uses 127.0.0.1
  port: number | null;
  container: string; // optional exact Docker name/ID for database CLI
  database: string; // PostgreSQL defaults to postgres, MongoDB to admin
}
interface Credentials { username: string; password: string }
interface UptimeCheck {
  id: string;
  serverId: string;
  url: string;
  intervalSeconds: number; // 15..86400
  enabled: boolean;
}
interface CheckResult {
  tsMs: number;
  ok: boolean;
  statusCode: number | null;
  addresses: string[];
  dnsMs: number | null;
  connectMs: number | null;
  tlsMs: number | null;
  ttfbMs: number | null;
  totalMs: number;
  error: string | null;
}
interface CheckView { check: UptimeCheck; history: CheckResult[] }
```

Absent fields mean unavailable, never zero. Numeric fields are finite JSON numbers. Times ending `Ms` use Unix milliseconds; byte counts use bytes; percentages range from 0 to 100 except Docker CPU, which can exceed 100 across cores. Snapshots are cached, and show their collection time. Disconnected remote data is explicitly labelled stale by the page. `includeHistory` defaults to false; only Capacity requests disk history. `summaryOnly` removes resource fields so overview polling does not transfer histories or detailed metadata. Pages request only their collectors and stop polling on unmount.

## Collectors, sources and fields

Each collector is registered in the existing Diagnostics panel as `infra:{collector}`. Resource health is separate from collector health: an operational collector can correctly report an unhealthy service.

| Collector | Source | Interval | Main resource fields |
| --- | --- | --- | --- |
| `mysql` | mysql/MariaDB `SHOW GLOBAL STATUS`, information_schema table sizes, `SHOW SLAVE STATUS\G` with modern `REPLICA` fallback | 30 s | connections, queryCounter, qps, slowQueries, cacheHitPercent, replicationLagSeconds, sizes |
| `postgres` | psql: pg_stat_database, pg_stat_activity, pg_database_size, pg_stat_replication and standby WAL positions | 30 s | same database fields; counterEpoch |
| `mongo` | mongosh: serverStatus, listDatabases, replSetGetStatus, existing system.profile | 30 s | same database fields; limitations |
| `rabbitmq` | curl GET localhost:15672/api/queues, then rabbitmqctl list_vhosts/list_queues | 30 s | depth, ready, unacknowledged, consumers, publishPerSecond, deliverPerSecond |
| `kafka` | kafka-topics and kafka-consumer-groups, localhost:9092 | 60 s | partitions, replicationFactor, currentOffset, logEndOffset, lag, consumer, consumers, consumePerSecond, producePerSecond |
| `haproxy` | stats socket via socat/nc, or localhost:8404/stats;csv | 30 s | weight, connections, rate, requestsTotal, errorsTotal, errorPercent, health |
| `nginx_upstreams` | nginx -T, attributable access/error log samples, TCP probes from server | 60 s | address, weight, backup, configuredDown, tcp, recentConnectionErrors, recentRequests, errorPercent |
| `traefik` | GET localhost:8080/api/http/services | 30 s | address, weight if supplied, service/backend health |
| `caddy` | GET localhost:2019/reverse_proxy/upstreams, server TCP probes | 30 s | activeRequests, failures, tcp |
| `docker_infra` | docker ps -a, selected inspect metadata, stats --no-stream | 30 s | cpuPercent, memory, memoryPercent, restarts, health, image, ports, composeProject |
| `kubernetes` | kubectl or k3s kubectl get nodes,pods -A -o json | 60 s | kind, node, restarts, readyContainers, reason; source state includes Pending |
| `certificates` | openssl x509 over live Let's Encrypt and configured nginx/Apache public certificate paths; nginx/Apache domain discovery | 1 h | expiresAtMs, daysRemaining, subject |
| `certificates_pc` | PC DNS, TCP 443, native Windows TLS with SNI; X.509 parsing | 6 h per name | expiresAtMs, daysRemaining, subject, verified, checkedAtMs |
| `uptime` | PC Rust DNS resolve and Windows curl.exe HTTP(S) GET | configurable | detailed `CheckResult` history via list_uptime_checks |
| `cron` | per-user crontab -l, /etc/crontab, /etc/cron.d, systemctl timer properties, journal/syslog | 5 min | user, source, schedule, command, timezone, nextRunMs, nextRun, lastRun, lastResult, exitCode |
| `capacity` | df -Pk plus local disk history | 5 min | usedBytes, totalBytes, usedPercent, growthBytesPerDay, daysUntilFull, rSquared, sampleCount, confidence, history |
| `ssh_security` | journal last 24 h, auth.log or secure fallback | 60 s | attempts, timeline: `{tsMs,count}[]` per hour |
| `ports` | ss -lntupH | 60 s | address, process; state exposed/local |
| `updates` | apt list --upgradable or dnf --cacheonly check-update and --security | 30 min | version, source; state update/security |
| `fail2ban` | fail2ban-client status and per-jail status | 60 s | banned, failed, bannedIps |
| `last_logins` | last -F -i -n 100 | 5 min | session |

Database `sizes` is `{database: string, bytes: number}[]`. MySQL hit rate uses cumulative InnoDB buffer-pool misses/requests. PostgreSQL uses buffer hits/(hits+reads). MongoDB estimates WiredTiger page hit ratio where counters are exposed. QPS is calculated only after two successful monotonic counter samples, with restart/reset checks. PostgreSQL labels the rate as transactions/s, MongoDB as operations/s. MySQL slow queries are cumulative above the configured server threshold; PostgreSQL counts currently active statements over one second; MongoDB counts already captured profile entries >=100 ms in the chosen database. No profiler or query logging is enabled. Replica lag remains unavailable on standalone instances or when the role lacks permission; an idle PostgreSQL standby with equal receive/replay positions reports zero.

HAProxy error percentage is cumulative response errors/sessions. Nginx error percentage uses only sampled access lines that contain the backend address. Without an attributable log format, the error rate remains unavailable. Traefik/Caddy configuration/status APIs do not provide a per-backend HTTP error rate; those cells remain unavailable with an explanation. Caddy passive failures are not treated as a request-error denominator or proof of HTTP health. TCP results establish reachability only. Explicit Nginx `down` stays down.

Concrete `server_name`, Apache ServerName/ServerAlias and application-discovered domains become suggestions automatically. Discovery and persistence run before the server-file probe, so missing OpenSSL or unreadable certificate files do not disable PC-side checks. `_`, regex names, wildcard names, variables and IP literals cannot identify a unique public SNI certificate and are excluded; add a concrete URL for them. Discovery is capped at 2,048 names per server. Public checks require no credentials. TLS trust/hostname failures produce critical status; expiry inspection then uses a separate unverified handshake so expired certificates can still show an expiry. Uptime always validates TLS. Warning is strictly below 21 days, critical strictly below 7 days (including expired).

Cron handles standard five-field schedules, aliases including @reboot, CRON_TZ, server timezone, Sunday 0/7 and Vixie day-of-month OR weekday semantics. Unsupported syntax and missing timezones have an explicit unknown next run. Per-user CRON_TZ resets at each file boundary. Most cron implementations do not log exit status; dispatch evidence is labelled as such. Systemd's Result and ExecMainStatus supply actual last-result evidence. Other users' unreadable crontabs produce a partial state.

## Timeouts and bounds

- Infrastructure runs alongside the existing system loop over the same persistent SSH session. Each collector has its own interval/error boundary; cancellation follows SSH disconnect/reconnect. Three infrastructure SSH commands may run concurrently. No collector waits on another collector's success.
- Remote GNU timeout terminates command groups after 10 seconds, with a one-second kill grace. Rust bounds channel reads at 12 seconds and channel open/close at 15 seconds overall. Each output is limited to 512 KiB, including stderr; truncation is reported. Dynamic shell values are single-quoted; errors are categorized without echoing command text or secrets.
- Most resource lists cap at 500, snapshots at 1,000 rows; disks cap at 256 mounts, cron at 500 rows plus 256 timers, RabbitMQ CLI at 32 vhosts, Nginx sampled logs at 24 files × 500 lines, TCP probes at 32 backends per sample. Backend rows without a completed probe retain unknown reachability. SSH authentication samples cap at 10,000 lines and 100 IPs; logins at 100. Broad deployments may show partial samples.
- PC work is limited to four shared workers including manual runs. Scheduling orders uptime checks by due time and reserves room for certificate work, so large sets do not starve later checks. Intervals are best effort; no background work runs while the app is closed. Rust DNS has a 4-second timeout. curl has 4-second connect and 10-second total limits, and a 1 MiB body cap; certificate TCP/TLS inspection has a 12-second total deadline shared by verified and fallback handshakes, with each I/O additionally capped at 3 seconds. Child curl windows are hidden.
- HTTP URLs must be absolute http(s), <=2,048 characters, with no embedded credentials or fragment. Checks record the initial GET response (2xx/3xx is healthy), without following redirects or environment proxies. DNS timing is from Rust; TCP and TLS durations subtract curl's preceding phases; server wait is first-byte minus pretransfer. Total includes the Rust lookup and GET. Averages and availability reflect recorded samples, not continuous reachability.
- At most 32 checks per server and 128 total. History is pruned on writes to 10,000 results per check and a 90-day cutoff; UI fetches the latest 240 and paints one selected chart. Paused checks can retain older results until another result is written. Disk samples deduplicate into five-minute buckets, are pruned on writes to 30 days, and are averaged hourly over the latest 30 days for regression (<=744 points per mount). Capacity changes suppress a forecast; a declining/flat trend does not predict exhaustion. At least six hourly samples spanning 24 hours are required; R² and observed span inform the confidence note.
- React tables paginate at 25 rows, with keyboard-focusable labelled scroll regions. Pages poll without overlapping requests, ignore stale responses after server changes/unmount, and use existing themed uPlot components and semantic status tokens.

## SQLite

Tables are added to the existing `metrics.db` at app setup, without changing existing migrations: `infra_settings`, `infra_domains`, `infra_checks`, `infra_check_results`, `infra_disk`. No credentials are stored in them. Deleting a check removes its history; deleting a server removes its infrastructure data. Data survives app restarts; live snapshots are rebuilt after reconnect.

## Verification

Fixture tests under `src-tauri/src/infra/fixtures/` cover each source parser, malformed/auth output, quoted CSV, IPv6 listeners, null replication, schedule semantics, expiry boundaries, forecast confidence, SQL round trips and command quoting. A loopback integration test exercises real Rust DNS plus PC curl GET for both HTTP 200 and 503. `docs/infra-ui-check.mjs` exercises the eight screens with mocked IPC in light/dark at 1,280 and 1,024 pixels, runs axe WCAG A/AA checks, verifies no page overflow/JS errors, and exercises pagination and uptime add/pause/remove and empty/error states. Install `playwright` and `axe-core` in a temporary verification directory, run the Vite server on port 1434, then pass that directory's `node_modules` to the script. This does not substitute for live VPS testing.

Source semantics: [PostgreSQL statistics](https://www.postgresql.org/docs/current/monitoring-stats.html), [RabbitMQ monitoring](https://www.rabbitmq.com/docs/monitoring), [HAProxy show stat](https://www.haproxy.com/documentation/haproxy-runtime-api/reference/show-stat/), [MongoDB serverStatus](https://www.mongodb.com/docs/manual/reference/command/serverstatus/), [Caddy API](https://caddyserver.com/docs/api), [curl timing fields](https://curl.se/docs/manpage.html).
