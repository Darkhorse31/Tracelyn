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
