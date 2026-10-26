# Tracelyn — Progress

## Rename (2026-10-23)

Product renamed from **ServerLens** to **Tracelyn** across Tauri config (`com.tracelyn.app`), Cargo/npm package names, UI, app data folder (`%LOCALAPPDATA%\Tracelyn`), keyring service, and docs. Spec/plan filenames unchanged.

## Step 1 — Shell, design system, server management, SSH connect (2026-10-23)

### Built

- **Tauri 2 + React + TypeScript (Vite)** desktop scaffold with product name Tracelyn.
- **Design system (spec §7a):** dark-first slate theme, blue primary accent, semantic status colors (`status-ok`, `status-warning`, `status-critical`, `status-info`), 8px-friendly spacing, Inter + JetBrains Mono, tabular numerals helper, shadcn/ui + Lucide, skeleton loading states.
- **App shell:** collapsible sidebar with server switcher and navigation placeholders, top bar with disabled time-range picker placeholder, Overview and Settings routes.
- **Server management UI:** add/edit/delete servers (name, host, port, user, key or password auth, key path, optional passphrase/password, labels); test connection from the form.
- **Overview:** responsive server card grid with connection status, OS info after connect, unhealthy-first sort.
- **Rust core:**
  - `config.json` under `%LOCALAPPDATA%\Tracelyn\` (no secrets).
  - **keyring** → Windows Credential Manager for password and key passphrase.
  - **russh** SSH: key + password auth, host key **trust-on-first-use** with change detection (`known_hosts.json`).
  - Tauri commands: `list_servers`, `list_server_statuses`, `upsert_server`, `delete_server`, `test_server_connection`, `connect_server`, `disconnect_server`.
- **`.gitignore`** for Node, Rust `target`, `dist`, logs, env files.

### Verified

| Check | Result |
|--------|--------|
| `npm run build` (tsc + vite) | **Pass** |
| `cargo check` in `src-tauri` | **Pass** (4 minor warnings: unused imports / dead code) |

### Not yet verified

- End-to-end `npm run tauri dev` / packaged app against a real VPS (not run in this session).
- Persistent SSH sessions, keepalive, and reconnect (planned Step 2).
- Host key change UX (connection fails if key changed; no in-app warning dialog yet).
- Light theme toggle (tokens support light `:root`; UI defaults to dark only).
- Placeholder nav items (Server, Web traffic, APM, Redis, Alerts) are disabled until later steps.

### Module layout (high level)

- Frontend: `src/components/layout`, `src/components/servers`, `src/pages`, `src/lib/api.ts`, `src/hooks/use-servers.ts`.
- Rust: `src-tauri/src/config`, `secrets`, `ssh`, `state`, `commands`.

## Step 2 — System collector, Overview and Server views (2026-10-23)

### Built

- **`ConnectionManager`:** one persistent **russh** session per connected server, SSH keepalive (30s), exponential backoff reconnect (1s → 60s cap), runtime status **connected** / **degraded** (amber) / **offline** (red after 1 minute disconnected).
- **System collector** (read-only allowlisted commands): `/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `/proc/net/dev`, `/proc/diskstats`, `df -P` every **2s**; `ps` top CPU/memory every **5s**; `ss -s` every **5s**; `systemctl is-active` for default service list every **15s**. CPU %, memory %, network and disk I/O rates computed in Rust.
- **Parsers + fixtures:** unit tests for `proc_stat`, `meminfo`, `loadavg`, `net/dev`, `diskstats`, `df -P`, `ps`, `ss -s`, `systemctl` (10 tests).
- **Event hub:** Tauri events `overview-metrics` and `server-metrics:{id}`, batched at **250ms** (≤4/s); `subscribe_*` / `unsubscribe_*` commands so background views do not receive updates.
- **Overview UI:** server cards with live CPU, memory, disk, load, sparklines; unhealthy / high-util servers sorted first.
- **Server view:** nav enabled; synced **uPlot** charts (CPU, memory, load, disk usage, disk I/O, network in/out), top processes table, services list, skeleton loading and empty/error states.

### Verified

| Check | Result |
|--------|--------|
| `cargo test` in `src-tauri` | **Pass** (10 parser tests) |
| `cargo check` in `src-tauri` | **Pass** |
| `npm run build` | **Pass** |

### Not yet verified

- Live metrics against a real Linux VPS over SSH (not run in this session).
- Crosshair tooltip polish across all uPlot panels (basic sync key wired).
- Service auto-detection beyond the static candidate list (full detection planned with web stack in Step 3).

### Module layout (additions)

- Rust: `connection/`, `collectors/system/`, `events/`, `metrics/`.
- Frontend: `src/pages/server-page.tsx`, `src/components/charts/`, `src/hooks/use-overview-metrics.ts`, `src/hooks/use-server-metrics.ts`.

## Steps 3–7 — Parallel branches merged and wired (2026-10-23)

Merged `feat/weblogs-apm`, `feat/redis`, and `feat/storage-alerts` into `master` (no-ff), removed agent scratch files, extended `.gitignore`, and connected collectors to **`ConnectionManager`**.

### Built (integration)

- **Web / APM:** `WebPipeline` detects nginx/Apache over SSH, parses config, streams access logs via `tail -F` on a dedicated channel, feeds `TrafficAggregator`, emits `web-traffic:{id}` and `apm-metrics:{id}` (batched with existing hub). UI shows permission-denied and missing-timing notices.
- **Redis:** when `redis-server` is present, `RedisCollector` polls via `redis-cli` on the SSH session; events `redis-metrics:{id}`; Redis password in keyring.
- **Storage:** live samples persisted for system, web, endpoint, and Redis metrics; SQLite rollups/prune on a background timer; `query_metric_series` for historical ranges.
- **Alerts:** default rules seeded; engine runs on live metric maps with Windows notifications on fire; Alerts page uses Rust API (`useAlerts` → Tauri).
- **Time range:** shared `TimeRangeProvider` + `TimeRangePicker` in top bar; live views subscribe to events; historical ranges query SQLite (Server CPU, web req/s, Redis series).

### Verified

| Check | Result |
|--------|--------|
| `cargo test` in `src-tauri` | **Pass** (34 tests) |
| `cargo check` in `src-tauri` | **Pass** (warnings: unused re-exports / demo command) |
| `npm run build` | **Pass** |

### Not yet verified

- End-to-end against a real Linux VPS with nginx/Apache logs, Redis, and alert firing (not run in this session).
- Custom time-range dialog (preset + disabled custom button only sets custom preset).
- Overview historical cards when not in Live mode (live subscription only).
- APM route-rules editor pushing rules into the live `RouteNormalizer` on the server.
- Packaged `npm run tauri build` release binary.

### Branch notes (archived)

- **Web traffic & APM core:** `webserver/` parsers, aggregation, virtualized log UI, APM heatmap/tables (see git history on `feat/weblogs-apm`).
- **Redis core:** `collectors/redis/`, fixtures, Redis page UI (see `feat/redis`).
- **Storage & alerts core:** `storage/`, `alerts/`, SQLite schema, notification plugin (see `feat/storage-alerts`).

## Quality pass (2026-10-26)

Integration polish on `main`: themes, accessibility hooks, chart theming, and build hygiene after feature merge.

### Built / improved

- **Themes:** `ThemeProvider` with Light / Dark / System, persisted in `localStorage` (`tracelyn-theme`), 200ms cross-fade (skipped when `prefers-reduced-motion`), Tauri window chrome sync. Switcher in top bar (`ThemeMenu`) and Settings (`ThemeRadioGroup` + Appearance card). `main.tsx` applies stored theme before mount (no forced dark).
- **Charts:** `src/lib/chart-theme.ts` — Okabe-Ito palette via CSS tokens, uPlot redraw on theme change, Viridis heatmap tokens, colorblind-safe multi-series dashes.
- **Accessibility:** WCAG-oriented token comments in `index.css`, global `:focus-visible` rings, `prefers-reduced-motion` overrides, chart `sr-only` summaries, table captions, icon `aria-label`s, alert toasts via `AlertAnnouncer` (Sonner live region).
- **Rust cleanup:** trimmed unused crate re-exports; Apache timing parser wired as fallback for `Custom` log format.
- **Settings:** Appearance section for theme preference alongside server management.

### Verified (this session)

| Check | Result |
|--------|--------|
| Merge conflict markers (`<<<<<<<`, etc.) in `src`, `src-tauri`, lockfiles | **None found** |
| `npm run build` (tsc + vite) | **Pass** (0 TS errors) |
| `cargo check` in `src-tauri` | **Pass** (0 warnings) |
| `cargo test` in `src-tauri` | **Pass** (39 tests) |
| `npm run tauri dev` | **Pass** — Vite on :1420, `tracelyn.exe` started; no Rust panics or errors in dev terminal output |
| `npm run tauri build` | **Pass** — see installer paths below |

**Release installers**

- `src-tauri/target/release/bundle/msi/Tracelyn_0.1.0_x64_en-US.msi`
- `src-tauri/target/release/bundle/nsis/Tracelyn_0.1.0_x64-setup.exe`
- Binary: `src-tauri/target/release/tracelyn.exe`

### Not yet verified

- Automated WCAG contrast ratio measurement (tokens designed for 4.5:1 / 3:1; not run through axe or similar in this session).
- Full manual keyboard walkthrough of every dialog, table, and sidebar control.
- WebView devtools console during `tauri dev` (terminal-only smoke test).
- Live SSH / nginx / Redis / alert firing on a real VPS.
- Responsive layout audit at exactly 1024px width.
- Custom time-range dialog, Overview historical cards in non-Live mode, APM route-rules live push (unchanged from Steps 3–7).
- Host key change in-app warning dialog.

## Applications, logs, and collector reliability (2026-10-26)

### Built

- **Application registry:** SSH discovery of PM2 (including other users' `PM2_HOME`), custom systemd units under `/etc/systemd/system`, Docker, nginx vhosts (`nginx -T`, nested `server` blocks), Apache (`apache2ctl -S` / `httpd -S` plus `CustomLog`), and known listening services. Domain/port mapping via `ss`. Manual add, edit, hide, and merge stored in SQLite `app_registry`. Rediscovery on demand and every 60s. Contract in `docs/apps-api.md`.
- **Applications UI:** sidebar section, searchable/groupable/sortable table (status, CPU, memory, restarts, uptime, req/s, error rate, p95), detail page with Overview / Logs / Requests / Errors.
- **Log explorer:** per-app and global pages. Ring buffer of 100k lines in Rust, incremental batches to the UI, virtualized list, level detection (JSON + plain text + stack grouping), filters (app, level, text, regex, exclude, time range, saved filters), pause/resume, follow, copy, JSON expand. Missing files and permission errors show a reason and a fix hint. Access logs are tailed per vhost (up to 24) and attributed to the owning app. Other sources use at most 8 SSH follow channels.
- **Redis:** explicit phases (not installed, auth required, connection refused, timed out, loading, live). Password and ACL username in Credential Manager. Port, unix socket, and `docker exec` container in config. 5s timeout and reconnect backoff. Snapshot replay after the listener is attached, so the page is not blank on first paint.
- **Web / APM:** every distinct access log from nginx/apache config. Missing `$request_time` / `%D` still shows req/s, status, and routes, with a copyable `log_format` banner. Subscribe-after-listen plus cached snapshot for the same first-paint race.
- **Diagnostics:** per-server panel for system, apps, PM2, systemd, Docker, nginx, Apache, Redis, and each log source.
- Parser tests cover multi-app PM2, 20 nginx vhosts, Docker JSON, Apache `-S`, systemd denylist, log levels, and Redis CLI shapes.

### Verified (this session)

| Check | Result |
|--------|--------|
| `cargo test` in `src-tauri` | **Pass** (51 tests) |
| `cargo check` in `src-tauri` | **Pass** (0 warnings) |
| `npm run build` | **Pass** (0 TS errors) |
| `npm run tauri build` | **Pass** |

**Release installers**

- `src-tauri/target/release/bundle/msi/Tracelyn_0.1.0_x64_en-US.msi`
- `src-tauri/target/release/bundle/nsis/Tracelyn_0.1.0_x64-setup.exe`
- Binary: `src-tauri/target/release/tracelyn.exe`

### Not yet verified on a real server

- The user's VPS: ~20 mixed-language apps under PM2, nginx in front, Redis auth/port/socket/Docker, and permission errors on log files.
- That a full 100k-line tail stays at 60fps in the WebView (virtualization is in place; not load-tested here).
- Windows Credential Manager prompts against a real Redis password (the save path is implemented; not exercised against a server).
- Automated WCAG contrast measurement and a full keyboard pass of the new tables and dialogs.

## Phase 2 — Deep APM and traces (2026-10-26)

### Built

- **OpenTelemetry file ingestion (read-only SSH).** Per-app and server-wide paths in SQLite (`otel_paths`). Suggestions from `/var/log/otel` and app directories are stored and confirmed by the user; they are not tailed until confirmed. Each poll reads at most 3 files, 256 KiB each, with byte offsets (`run_command_bytes`). Partial JSON is held up to 256 KiB. Truncation resets the offset and skips the already-replaced bytes. Gzip-rotated files are skipped and counted as malformed. Malformed lines and file errors show on the `otel` diagnostics row.
- **Traces.** Spans, linked OTLP logs, and error fingerprints in SQLite. Retention follows the existing raw history window, plus caps (100k spans per server, 200k globally, 20k OTLP logs, 5k error issues, 2k slow-query groups). Traces page filters by service, operation, status, minimum duration, trace id, time range, and app. Detail page is a canvas waterfall (visible rows only, parent indent, hatched critical path, “error” label) with attributes, events, exception stacks, and links to logs and errors. Detail is capped at 5,000 spans.
- **Service map.** Built at query time from client/server/producer/consumer spans and peer attributes (rate, error rate, p95). Map view plus a keyboard table. Clicking a service opens Traces filtered to that service. The newest 20,000 spans in the window are used.
- **Errors.** Stack fingerprints from OTel exception events and from application log lines (Node, Python, Go panic, Java, PHP, .NET). Line numbers, addresses, and ids are stripped. Issues have first/last seen, count, 24-hour sparkline, app, sample, and example traces. Resolve / ignore / reopen is stored in SQLite. Resolved issues reopen when the same fingerprint is seen again; ignored issues stay ignored. Errors page and the application Errors tab.
- **Slow queries.** MySQL/MariaDB slow log (including the legacy `YYMMDD HH:MM:SS` time line), PostgreSQL `duration:` / `execute` lines, MongoDB 4.4+ slow-op JSON. Paths are detected read-only from `my.cnf`, `postgresql.conf`, and `mongod.conf`, with a manual path that detection does not replace. Literals become `?`. Groups show count, total, average, p95, rows examined, and last seen. Each engine card has the setting to turn logging on, with a copy button. A database app is linked when exactly one application name matches that engine.
- **Links.** An API endpoint row links to Traces filtered by that operation. A trace links to logs (`?q=` trace id) and to errors in that trace. An error group links to example traces and related logs.
- **Setup panel.** Copyable Collector file-exporter config plus Node.js, Python, Go, Java, PHP, and .NET snippets that export OTLP/HTTP to `127.0.0.1:4318`. Shown on Traces and on each application overview.
- **Diagnostics.** Collectors `otel` and `slowlog` with status and a fix hint, included in the existing diagnostics panel.

### Verified

| Check | Result |
|--------|--------|
| `cargo test` in `src-tauri` | **Pass** (68 tests) |
| `cargo check` in `src-tauri` | **Pass** (0 warnings) |
| `npm run build` | **Pass** (0 TS errors) |
| `npm run tauri build` | **Pass** |

**Release installers**

- `src-tauri/target/release/bundle/nsis/Tracelyn_0.1.0_x64-setup.exe`
- `src-tauri/target/release/bundle/msi/Tracelyn_0.1.0_x64_en-US.msi`
- Binary: `src-tauri/target/release/tracelyn.exe`

### Not verified / gaps

- No browser tools in this session, so the new screens were not clicked through in light, dark, or system theme, and 60fps was not measured in the WebView. The waterfall paints only the visible canvas rows and skips `requestAnimationFrame` when reduced motion is set.
- No real VPS. Detection, tailing, permission errors, and rotation were not exercised over SSH.
- No automated WCAG contrast measurement.
- A MySQL statement split across two polls is kept only until the entry ends with `;` or a blank line. A buffer over 64 KiB is counted malformed and dropped.
- When a file shrinks (truncation), the current smaller contents are not re-parsed, so a replaced file can skip the lines already in it. The next append is read. Span upserts would have been idempotent; errors and slow queries would not.
- Optional OTel metrics are stored as `otel.<name>` raw samples only. There is no metrics UI for them.
- Service map and trace list are computed in SQLite at query time (list capped at 200 traces).

## Phase 3 — Infrastructure breadth (2026-10-26)

### Built

- **Self-contained infrastructure modules:** Rust in `src-tauri/src/infra/`, views in `src/pages/infra/`, components and IPC hook in `src/components/infra/`. Shared changes add command registration, managed infra state, one SSH lifecycle integration, diagnostics, sidebar/routes, server deletion cleanup, and summary cards. Existing uncommitted work was preserved; no commit was created.
- **Databases:** MySQL/MariaDB, PostgreSQL and MongoDB status, connections, counter-derived throughput, slow-query evidence, cache efficiency, replica lag and database sizes. Native CLI or Docker, optional targets, credentials in Windows Credential Manager. PostgreSQL rate is transactions/s; slow-count scopes are explained per engine. Missing metrics remain unavailable. MySQL supports legacy and modern replica-status spellings.
- **Queues:** RabbitMQ localhost management API with CLI/vhost fallback; depths, unacked messages, consumers and API rates. Kafka topics, partitions, replicas, consumer-group offsets/lag, and production/consumption rates derived from successive offsets.
- **Load balancers:** HAProxy socket/CSV stats; Nginx upstream/direct target parsing, weights, explicit down state, attributable sampled log errors, server-side TCP checks; Traefik localhost service/backend status; Caddy localhost upstream gauges plus TCP checks. API sources that do not expose backend weights or error rates explicitly show unavailable values.
- **Containers:** selected Docker inspect metadata, one-shot CPU/memory stats, health, restart count, ports and Compose project; kubectl/k3s nodes and pods, readiness, restarts and Pending state.
- **Certificates and uptime:** server-side public certificate file expiry and PC-side DNS/TLS/SNI inspection for discovered concrete domains. Strict warning <21 days, critical <7 days, and critical on public TLS trust/hostname failure. Add/edit/pause/run/remove PC HTTP(S) checks, configurable 15s–24h intervals, DNS/TCP/TLS/server-wait/total timings, SQLite history and automatic domain suggestions.
- **Cron:** readable crontabs for all enumerated users, /etc/crontab and /etc/cron.d, systemd timer next/last runs and service results. Standard cron schedules, aliases, CRON_TZ/server timezone and day-of-month OR weekday behavior. Dispatch-only evidence is not presented as a successful exit.
- **Capacity:** per-mount disk samples in five-minute buckets, hourly averages, 30-day linear regressions, days until full and R²/span confidence notes. Insufficient history, resizing and non-growing trends have explicit states. Historical chart uses the existing chart-theme/uPlot components.
- **Security:** failed SSH events, top IPs and hourly timeline; listening TCP/UDP sockets and owners, with wildcard-bind flags; cached apt/dnf pending/security updates; fail2ban jails/banned IPs; recent login sessions.
- **UI and diagnostics:** eight Infrastructure sidebar pages, summary cards on Overview and Server, 21 collector diagnostics. Themed semantic badges, loading/empty/error/fix states, bounded/paginated tables, labelled keyboard-scrollable table regions, and stale/offline notices. Pages request only their collectors; summaries omit detailed resource fields and history.
- **Reliability:** independent collector loops on the existing SSH connection, three infrastructure SSH channels, per-command time/output limits, cancellation on disconnect, four shared PC workers with fair check ordering and capacity for certificate work. Credentials and full remote commands never enter error messages. Check/server deletion removes infrastructure history and credentials where applicable.
- **Contract and repeatable browser check:** `docs/infra-api.md` documents commands, types, sources, field semantics, intervals and limits. `docs/infra-ui-check.mjs` runs mock-IPC browser/a11y/interaction checks without contacting a server.

### Verified

| Check | Result |
| --- | --- |
| `cargo test` | **Pass — 91 tests**, including every infrastructure source parser, cron/timezone semantics, expiry boundaries, regression/resize cases, SQLite round trips, shell quoting, absolute TLS deadlines, Windows Credential Manager save/read/delete, and real loopback DNS/curl GET (200 and 503) |
| `cargo check` | **Pass**, no Rust compiler warnings |
| `npm run build` | **Pass**, no TypeScript errors |
| Browser smoke + axe WCAG A/AA rules | **Pass — 32 screen/theme/width combinations**, 8 pages × light/dark × 1280/1024 px, zero reported violations, page overflow or JavaScript exceptions |
| Browser interactions | **Pass** — table pagination, uptime add/pause/resume/remove, unavailable-source and IPC-error states |
| `npm run tauri build` | **Pass** in the merged main workspace; both Windows installers verified (see merge record below) |

### Practical limits / not verified

- No live VPS was supplied. Remote CLI versions, role permissions, Docker/kubeconfig access and database authentication were not exercised against a real server. Windows Credential Manager storage was verified with a temporary isolated entry that was then removed. Fixtures and the loopback HTTP integration test verify the parsing and PC GET paths.
- Automated axe checks cover detectable rules in the tested main content; they do not certify complete WCAG conformance or replace a full assistive-technology review. Browser tests use mocked Tauri IPC, rather than a native WebView automation session.
- A 10-server memory/CPU soak and measured 60fps native WebView run were not performed. Resource responses, polling, PC concurrency and table rendering are explicitly bounded.
- Source limitations are visible: Traefik/Caddy APIs lack per-backend HTTP error counters; Nginx log error attribution needs upstream addresses in existing logs; Mongo slow counts need existing profiling data; many cron logs omit exit results. No server logging, API configuration, profiler, package cache or permissions are changed automatically.
- Wildcard/regex/variable vhost names need concrete DNS names. Certificate inspection targets current live/configured public certificates and concrete domains; archive/private-key files are not transferred. Broad deployments can reach the documented sample limits.
- Kafka uses existing local CLI authentication/configuration at localhost:9092. RabbitMQ CLI sampling covers up to 32 vhosts; management API sampling covers up to 500 queues. TCP probes cover up to 32 backends per sample; remaining reachability stays unknown.
- Package updates reflect existing metadata and can be stale. PC checks stop when Tracelyn closes and run at best-effort intervals under load. Windows curl.exe must be available.
- Existing build tooling emits a Vite native-config warning (`__dirname`), a large frontend-chunk advisory and a Tauri `.app` identifier advisory. These are separate from compiler/lint errors; no application identifier was changed in this merge-focused phase.


## Phase 4 — Operations (opt-in) (2026-10-26)

### Built

- **Operations mode** is off by default (`app_settings.operations_mode`). While it is off, restart/reload/stop/start and the SSH terminal stay disabled and the backend rejects them. The switch and the per-server sudo password live on Settings. The password is stored in Windows Credential Manager (`server:{id}:sudo_password`) and is removed with the server.
- **Guarded actions** for PM2 (start/stop/restart/reload, with `PM2_HOME` and `sudo -u` when the process owner is not the SSH user), systemd, and Docker. Nginx runs `nginx -t` and reloads only when the test exits 0. Apache does the same with `configtest` then `graceful`. The confirm dialog shows the exact command. Stop requires the name to be typed. The same checks run again in Rust before anything is sent.
- **Health check** after the command, for 1–60 seconds (default 15). Start/restart/reload must report online; stop must report not online. Otherwise the dialog and a toast show a warning. A failed config test does not reload and does not count as healthy.
- **Audit log** in SQLite (`audit_log`, capped at 2,000 rows): Windows user, server, target, command, time, exit code, 500-character output excerpt. The Audit page filters by server, target, time, and failures, and exports CSV. Sudo passwords are not stored; if one appears in output it is redacted.
- **Terminal:** xterm.js tabs (up to 8) on a russh PTY (`xterm-256color`), resize, Ctrl+Shift+C / Ctrl+Shift+V and right-click paste, light and dark colors from the app theme, 2,000 lines of scrollback. Quick actions on the application list, application detail, server service rows, and alert history: restart or reload, open logs, open terminal.
- Shells and actions use a second russh connection with the same saved credentials and host-key store. The collector owns the monitor session's `Handle`, and that type is not `Clone`.

### Verified

| Check | Result |
|--------|--------|
| `cargo test` in `src-tauri` | **Pass** (82 tests, 0 warnings) |
| `cargo check` in `src-tauri` | **Pass** (0 warnings) |
| `npm run build` | **Pass** (0 TS errors) |
| `npm run tauri build` | **Pass** |

**Release installers**

- `src-tauri/target/release/bundle/nsis/Tracelyn_0.1.0_x64-setup.exe`
- `src-tauri/target/release/bundle/msi/Tracelyn_0.1.0_x64_en-US.msi`
- Binary: `src-tauri/target/release/tracelyn.exe`

### Not verified / gaps

- No real VPS. Commands, sudo, health checks, and the PTY were not run against a server.
- No browser tools that can drive the Tauri webview, so the new dialogs, audit filters, and terminal were not clicked through in light or dark theme.
- Target names are limited to letters, digits, and `. _ : @ - /`. A name with a space or shell metacharacter is rejected.
- Nginx and Apache actions test and reload the server config. They do not reload a single vhost.
- Vite still notes that the main JS chunk is over 500 kB. That includes xterm on the terminal route.


## Phase 3 + Phase 4 merge (2026-10-26)

- Merged infrastructure from `serverlens-p3` into the main workspace, preserving Phase 4 operations, terminal, audit, sudo credentials and all existing Phase 2 work. No commits were created; the Phase 5 workspace was not accessed.
- Combined managed infrastructure state and all IPC registrations, SSH polling, diagnostics, server deletion cleanup, overview/server summaries, routes, and separate Infrastructure/Operations sidebar sections.
- Kept Phase 4 npm dependencies/lockfile and added the Phase 3 Rust dependencies/lockfile. Both phases use additive, idempotent SQLite table creation; infrastructure tables are initialized at app setup, and the existing insight/operations migrations and applied schema meanings are unchanged. No numbered migration collision exists.
- Main-workspace verification: `cargo test` passed with **105 tests**, no failures or Rust warnings; `cargo check` passed without Rust warnings; `npm install` completed with the Phase 4 manifest/lockfile unchanged; `npm run build` passed. `npm run tauri build` passed and produced both Windows installers.
- The production frontend passed the infrastructure browser check across **32 screen/theme/width combinations** (eight pages, light/dark, 1280/1024 px): zero axe WCAG A/AA violations, horizontal overflow or JavaScript exceptions. Pagination, uptime add/pause/remove and unavailable/error states passed. Separate smoke checks verified Infrastructure/Audit/Terminal navigation and exact active states in both themes.
- Hardened `docs/infra-ui-check.mjs` to await fonts, inherited heading color and finite animation completion before the contrast audit. This avoids recording transient theme colors; no accessibility rules are disabled.
- Existing Vite native-config/chunk-size and Tauri identifier advisories remain. No live VPS checks were performed.

### Merged release artifacts

- NSIS installer: `src-tauri/target/release/bundle/nsis/Tracelyn_0.1.0_x64-setup.exe`
- MSI installer: `src-tauri/target/release/bundle/msi/Tracelyn_0.1.0_x64_en-US.msi`
- Application: `src-tauri/target/release/tracelyn.exe`

All four required checks passed in main. The final integrity comparison preserved 284 pre-existing files byte-for-byte outside the 11 shared files intentionally merged; all 41 Phase 3 additions are present. The browser checker is the only Phase 3 addition adjusted during integration.

## Phase 5 — Teams and intelligence (2026-10-26)

Code lives in `src-tauri/src/intel`, `src/pages/intel`, and `src/components/intel`. Shared files only gained the module, command registration, the Intelligence route, the sidebar item, and two Cargo dependencies (`native-tls`, `x509-parser`). Intel tables are created with `CREATE IF NOT EXISTS` on first use, so `storage/schema.rs` was not edited. The built-in alert engine is unchanged.

### Built

- **Rules.** Extra kinds on top of the existing duration and hysteresis engine: any stored metric, log-pattern rate (regex, per-minute, line ids deduped), app error rate, p95, SSL certificate days remaining, and TCP uptime. A rule stays hot for `duration_secs` before it fires. Clearing waits `clear_duration_secs` (0 clears immediately). High rules fire at or above the threshold and clear below the clear level (default 95% of the threshold). SSL and uptime fire at or below the threshold. Missing metric keys are skipped. Live evaluation does not open the network; SSL and uptime probes run on a 60-second maintenance tick, and only when the last check is older than five minutes.
- **Channels.** Windows toast, Slack, Discord, Microsoft Teams, Telegram, SMTP email, and a generic HTTPS webhook. Webhook URLs, bot tokens, and SMTP passwords are stored in Windows Credential Manager under service `Tracelyn`, account `intel-channel:{id}`. HTTPS posts go through `curl.exe` (`--proto =https`, no window). SMTP is a small client (STARTTLS on 587, implicit TLS on 465, or plain) with AUTH PLAIN. Each channel has a test button. Outbound work is queued (cap 50, oldest dropped) and flushed every two seconds so the SSH poll loop does not wait on HTTP.
- **Anomalies.** Per hour-of-week EWMA (alpha 0.2) plus a robust z-score from the MAD of the last 48 residuals. A point is anomalous after 8 samples when |z| ≥ 3.5. Watched series: CPU, memory, disk, load, request rate, HTTP 5xx rate, endpoint p95, app error rate, and app p95. The Anomalies tab draws the value, the baseline, and the band. Events are stored and cool down for five minutes per metric.
- **Incidents.** A new alert joins an open or acknowledged incident on the same server when that incident was updated within 30 minutes. Severity only escalates. Timeline entries are alerts, PM2 restart increases (the first observation is silent), anomalies, notes, and status changes. Acknowledge, resolve, and notes are on the incident page. Existing `alert_events` rows are claimed once and folded into the same incidents and channels.
- **Sharing.** An incident or the anomaly chart exports as self-contained HTML (inline CSS, escaped text, inline SVG sparkline, no script) or a small multi-page Helvetica PDF. Config export/import is JSON version 1: servers, route rules, Redis targets, built-in alert rules, intel rules, channels, saved views, and digests. Secrets are omitted. Import merges by id and does not delete servers or touch Credential Manager. The bundle is capped (2 MB; 500 servers and rules; 50 channels; 200 views; 20 digests).
- **Digest.** Daily or weekly, at an hour UTC and a weekday, sent to one channel. A failed attempt retries after one hour. Send now ignores the schedule. The digest runs only while the app is open.
- **UI.** Sidebar item Intelligence (`/intel`) with tabs for incidents, rules, channels, anomalies, digest, and sharing. Incident detail is `/intel/incidents/:incidentId`. Light and dark tokens, empty, loading, and error states, and a screen-reader caption on the anomaly chart.

### Verified

| Check | Result |
|--------|--------|
| `cargo test --lib` in `src-tauri` | **Pass** (98 tests, 0 failed) |
| `cargo check` in `src-tauri` | **Pass** (0 warnings) |
| `npm run build` | **Pass** (0 TS errors; also the `beforeBuildCommand` of the installer build) |
| `npm run tauri build` | **Pass** (0 warnings; release compile 11m 18s) |

**Release installers**

- `src-tauri/target/release/bundle/nsis/Tracelyn_0.1.0_x64-setup.exe` (4.19 MiB)
- `src-tauri/target/release/bundle/msi/Tracelyn_0.1.0_x64_en-US.msi` (5.62 MiB)
- Binary: `src-tauri/target/release/tracelyn.exe`

### Not verified / gaps

- No browser tools in this session, and the Intelligence screens call Tauri commands, so they were not clicked through in the WebView. Theme, keyboard, and contrast were not measured. `tsc` and the production Vite build are the checks that ran.
- No live Slack, Discord, Teams, Telegram, SMTP, or webhook endpoint was contacted. URL and payload construction is unit-tested. Delivery needs `curl.exe` on PATH for HTTPS channels.
- SSL expiry and SMTP were not exercised against a live server. A certificate probe needs a successful TLS handshake. TCP uptime treats connect and DNS failure as down.
- Enabling a Windows intel channel and a built-in alert rule can toast twice for the same built-in firing: the old notifier and the channel. Intel-only rules do not write `alert_events`.
- The seasonal baseline updates on every sample with alpha 0.2, so last week's level fades within about an hour. It is not a once-per-hour week-to-week memory.
- The digest scheduler runs inside the app process. It does not fire while Tracelyn is closed.
- PDF text is Helvetica only. The sparkline is in the HTML report.


## Phase 5 merge into main (2026-10-26)

- Merged all 20 Phase 5 additions and combined shared command registration, routes, navigation, scheduling, and live alert wiring with Phases 3 and 4. Kept dedicated Infrastructure, Operations, and Teams and intelligence sidebar sections. No commits were created.
- Retained main's `native-tls` and `x509-parser` 0.18 dependencies and Cargo lockfile; Phase 5 requested the already-present TLS libraries at an older parser version. Phase 5 added no npm dependencies; main's npm manifest/lockfile retain the Phase 4 xterm packages, so no dependency install is needed.
- Intel uses its own `intel_*` SQLite tables with additive, idempotent creation on first use. There are no numbered migrations to renumber; existing infrastructure, insight, and operations migrations retain their meaning.
- Main-workspace verification: `cargo test` passed with **135 tests**, no failures or Rust warnings; `cargo check` passed without Rust warnings; `npm run build` passed; `npm run tauri build` passed (release compile 8m 37s, both Windows installers produced). All four required checks passed in main. Source-workspace results above describe the original Phase 5 verification.
- Added `docs/intel-ui-check.mjs` using mocked Tauri IPC. The final production frontend passes **56 Intelligence cases** (six tabs, incident detail, rule/channel dialogs, populated/empty/error states, light/dark, 1280/1024 px) and the existing **32 infrastructure cases**, with zero axe WCAG A/AA violations, horizontal overflow, or JavaScript exceptions. Incident acknowledge/note/resolve and tab ArrowLeft/ArrowRight/Home/End, focus, and a single tab stop pass in both themes. No live delivery or VPS actions were performed.
- Accessibility integration fixes: Intelligence tabs now use roving focus, arrow/Home/End keys, and one persistent labelled panel; the import control has a visible keyboard focus ring. Slightly darkened the existing light-theme success token after the full-shell audit measured the Online badge below 4.5:1 on its sidebar tint. The dark theme and the rest of the design tokens are unchanged.
- Integrity checks preserve all **77 existing IPC registrations**, add all **25 Intelligence commands** without duplicates, and preserve all existing routes alongside `/intel` and `/intel/incidents/:incidentId`. All **20 Phase 5 additions** are present; two received the accessibility fixes above. Of **336 pre-existing source/config/docs files**, **329 remain byte-for-byte identical**; the seven intentional edits are progress docs, app routes/sidebar, the success color, Rust module/command registration, and live alert integration. Git HEAD remains `d52a5a6`.
- Existing Vite native-config/chunk-size and Tauri identifier advisories remain. No new Rust warnings were introduced. The build and browser checks used the final frontend assets, including the accessibility fixes.

### Merged Phase 3 + Phase 4 + Phase 5 release artifacts

- NSIS installer: `src-tauri/target/release/bundle/nsis/Tracelyn_0.1.0_x64-setup.exe` (4.60 MiB)
- MSI installer: `src-tauri/target/release/bundle/msi/Tracelyn_0.1.0_x64_en-US.msi` (6.23 MiB)
- Application: `src-tauri/target/release/tracelyn.exe`

## Setup guide and requirements (2026-10-26)

### Built

- **Setup guide** (`/setup`). One read-only SSH session runs `id`, `sudo -n -l`, `find` on nginx/apache logs, `journalctl -n 1`, PM2 home and log readability, the Docker socket mode, and `redis-cli ping`. It does not call `usermod`, `chmod`, package managers, or service restarts. Each row is pass, warning, fail, or not checked, with a plain explanation, the copyable command (SSH username filled in), and Re-check. Re-check all and a single row both refresh the set. OpenTelemetry paths and each discovered app log path are tested with `test -r`. MySQL, PostgreSQL, MongoDB, and RabbitMQ use saved targets/credentials plus the last infrastructure sample. Linked from the empty Overview, Diagnostics on each server, and the Help section of the sidebar.
- **How to fix.** Permission and access errors (web access logs, log sources, diagnostics, application log sources, infrastructure permission/auth, Redis auth, the empty API list) include a button that opens the matching guide step.
- **How Tracelyn works** (`/how-it-works`). Connect, discover apps, monitor, alerts, operations. Linked from Overview onboarding, the Setup guide, and Help in the sidebar.
- **Listener duplicates.** IPv4 and IPv6 sockets for the same process and port become one service app (`sshd:22`, `postgres:5432`) and one security-port row. Addresses are kept together. The application list dedupes the same way.
- **Log levels.** Lines stay UNKNOWN only when nothing in the text or stream says otherwise. Keywords (including ERR, FATAL, CRIT, PANIC, Exception, Traceback, WARN, NOTICE, INFO, DEBUG, TRACE), JSON `level`/`severity`, syslog `<pri>`, HTTP 5xx/4xx, and stderr (error, unless the text is already a warning or info) are classified.
- **Private key picker.** Add/Edit server keeps a typeable path and adds Browse (native dialog, starting in `%USERPROFILE%\.ssh`, all files) plus a dropdown of `id_ed25519`, `id_rsa`, `id_ecdsa`, `*.pem`, and `*.ppk`. Public keys, missing files, and PuTTY `.ppk` files get a specific message, including the PuTTYgen OpenSSH export step. `tauri-plugin-dialog` is registered with the dialog capability.

### Verified

| Check | Result |
|--------|--------|
| `cargo test` in `src-tauri` | **Pass** (143 tests, 0 failed) |
| `cargo check` in `src-tauri` | **Pass** (0 warnings) |
| `npm run build` | **Pass** (0 TS errors) |
| `npm run tauri build` | **Pass** (release compile 6m 47s) |

**Release installers**

- `src-tauri/target/release/bundle/nsis/Tracelyn_0.1.0_x64-setup.exe` (4.68 MiB)
- `src-tauri/target/release/bundle/msi/Tracelyn_0.1.0_x64_en-US.msi` (6.35 MiB)
- Binary: `src-tauri/target/release/tracelyn.exe`

### Not verified

- No live VPS. The setup probe was not run against a server. Classification is unit-tested from fixture output.
- The new screens were not clicked through in the WebView. Light/dark theme and keyboard behavior use the existing tokens, labels, and native controls.
- Vite still warns about `__dirname` in the config and a JS chunk over 500 kB. Tauri still notes that the bundle identifier ends in `.app`.
