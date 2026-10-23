# Tracelyn — Design Spec

Date: 2026-10-23
Status: Approved 2026-10-23

## 1. Goal

A Windows desktop app that monitors one or more Linux VPS servers over SSH, with nothing installed or running on the servers. One place to see system health, Nginx/Apache web traffic, per-API-endpoint performance (APM-style), Redis, and (phase 2) Datadog-style request traces.

Priorities: performance, smooth UX, clean and visually symmetric design.

## 2. Non-goals

- No agent, daemon, or service installed on servers.
- The app never changes server configuration. Where a config change would unlock data (e.g. response-time logging), it shows the exact change for the user to apply.
- No cloud backend, no accounts. All data stays on the user's PC.

## 3. Stack

- **Tauri 2** desktop shell (Windows first).
- **Rust core**: SSH (russh), log parsing, aggregation, local storage (SQLite via rusqlite), alerts.
- **React + TypeScript UI**, Tailwind + shadcn/ui, canvas-based charts (uPlot or ECharts canvas renderer).

## 4. Architecture

```
React UI  <--Tauri events/commands-->  Rust core
                                         ├─ ConnectionManager (one SSH session per server, keepalive, reconnect w/ backoff)
                                         ├─ Collectors (system, services, webserver, redis, traces)
                                         ├─ Parsers (nginx, apache, OTel JSON)
                                         ├─ Aggregator (rates, percentiles, route grouping)
                                         ├─ Store (SQLite: raw 7d, 1-min rollups 90d, configurable)
                                         └─ Alerter (Windows notifications)
```

Each unit has one job and a typed interface; collectors emit typed samples, the aggregator turns samples into ready-to-draw series, the UI only renders.

## 5. Connections and security

- Auth: SSH key (with optional passphrase) or password, per server.
- Secrets stored in Windows Credential Manager; server list (host, port, user, key path, labels) in a local config file without secrets.
- Host key verification (trust on first use, warn on change).
- Commands executed come from a fixed, read-only allowlist.
- Long-running streams (`tail -F` on logs) multiplexed as channels on the same SSH session.

## 6. Collectors (phase 1)

| Collector | Source | Interval |
|---|---|---|
| System | `/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `/proc/net/dev`, `/proc/diskstats`, `df -P` | 2 s |
| Processes | `ps` top by CPU/mem | 5 s |
| Services | `systemctl is-active` for detected services (nginx, apache2/httpd, redis, mysql, php-fpm, etc.) | 15 s |
| Connections | `ss -s` | 5 s |
| Web server | auto-detect nginx vs apache (process + config), locate access/error logs from config, `tail -F` stream | live |
| Redis | `redis-cli INFO`, `SLOWLOG GET`, `INFO keyspace` (auth from Credential Manager if needed) | 5 s |

Web server log handling:
- Parse common/combined and custom formats; detect whether response time is present (`$request_time`/`$upstream_response_time` for Nginx, `%D` for Apache).
- If timing is missing, endpoint latency panels show a notice with the exact one-line log_format change.

## 7. Views

1. **Overview** — card grid, one per server: status (online/degraded/offline), CPU, memory, disk, req/s, 5xx rate, sparklines. Unhealthy servers sorted first and highlighted.
2. **Server** — live charts for CPU, memory, load, disk usage and I/O, network; top processes; services status.
3. **Web traffic** — req/s, status code breakdown, top URLs, top IPs, bandwidth, live filterable log stream (by status, path, IP, text).
4. **API endpoints (APM-style)** — requests grouped by normalized route (numeric IDs, UUIDs, hashes collapsed to `:id`); per endpoint: throughput, avg/p50/p95/p99 latency, error rate, status breakdown, time-series chart; slowest and most-failing tables; click-through to matching log lines. User can add manual route rules.
5. **Redis** — ops/s, memory used/peak, hit rate, clients, keys per DB, evictions/expirations, replication role, slow log.
6. **Alerts** — rule list and history.
7. **Settings** — servers, retention, intervals, alert thresholds, theme.

Server switcher always visible in the sidebar; time range picker (live, 15m, 1h, 24h, 7d, custom) shared across views.

## 7a. Visual design (must feel professional, on par with Datadog / Grafana)

- Dark theme by default (light theme optional), neutral slate background, one accent color, semantic colors only for state: green ok, amber warning, red critical, blue info.
- 8 px spacing grid, consistent card sizes and gutters, aligned grids, symmetric layouts; no cramped or uneven panels.
- Typography: Inter for UI, JetBrains Mono for numbers, logs, and paths; tabular numerals so live values don't jitter.
- Charts: canvas-based, smooth line/area with subtle gradient fills, thin gridlines, crosshair tooltip synced across charts on the same view, consistent axis formatting (units like ms, req/s, %, GB), legends that toggle series, no chart junk.
- Specialized visuals: latency heatmap and p50/p95/p99 bands for endpoints, stacked status-code bars, flame graph for traces (phase 2), sparklines on overview cards.
- Motion: short (150–200 ms) easing on transitions, no flashing on data updates; skeleton loaders instead of spinners.
- Empty, loading, and error states designed for every panel.
- Icons: Lucide, single stroke weight.

## 8. Alerts

Windows notifications plus in-app history. Default rules (editable): server unreachable > 1 min, CPU > 90% for 5 min, memory > 90% for 5 min, disk > 85%, 5xx rate > 5% for 2 min, endpoint p95 above user threshold, Redis memory near maxmemory.

## 9. Storage

SQLite in the app data folder. Raw samples kept 7 days, 1-minute rollups 90 days (both configurable). Log lines are not stored in full; only aggregates plus a capped recent buffer for the live stream.

## 10. Performance

- All parsing and aggregation in Rust; UI receives batched, pre-aggregated updates (max ~4 UI updates/s per view).
- Views not on screen pause their subscriptions.
- Canvas charts; virtualized log list.
- Target: < 150 MB RAM and < 3% CPU with 10 servers connected, idle UI.

## 11. Error handling

- Connection loss: card turns amber, exponential backoff reconnect (1s → 60s cap), red after 1 min.
- Permission denied on logs or redis-cli: the panel states which file/command failed and the permission needed (e.g. add user to `adm` group).
- Unknown log format: offer a format editor with live preview against sample lines.

## 12. Phase 2 — Distributed traces (Datadog-style APM)

- User adds OpenTelemetry SDK / auto-instrumentation to their apps (PHP, Node.js, Python, etc.), configured with a file exporter writing OTLP JSON to a rotating file on the server.
- No extra service on the server; Tracelyn tails the trace file over SSH.
- Views: trace list (filter by service, endpoint, duration, error), flame graph per trace, time split (DB / Redis / external HTTP / own code), slow queries, errors with stack traces, service map.
- Setup guide per language shown in-app.

## 13. Testing

- Rust unit tests for parsers (fixture log files for nginx/apache variants), route normalization, percentile math, rollups.
- Integration test against a Docker container running sshd + nginx + redis.
- UI component tests for key views; manual perf check with 10 simulated servers.

## 14. Build order (phase 1)

1. Tauri + React shell, design system, server management, SSH connect (key + password).
2. System collector + Overview and Server views.
3. Web server detection, log streaming, Web traffic view.
4. API endpoints view.
5. Redis view.
6. Storage/history + time range picker.
7. Alerts.
Then phase 2 traces.

## 15. Workflow

All code is written through Cursor CLI on the user's Windows machine; COn plans, reviews, designs, and checks quality, and does not write app code itself.
