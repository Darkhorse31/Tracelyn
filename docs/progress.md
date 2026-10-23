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
