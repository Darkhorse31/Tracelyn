# Tracelyn — Redis monitoring core (parallel branch)

Date: 2026-10-23  
Scope: Step 5 core only (no ConnectionManager / SSH wiring).

## Rust

- New module `src-tauri/src/collectors/redis/`:
  - `RemoteCommandRunner` trait + `RedisCollector` polling `INFO all`, `SLOWLOG GET 128`, `INFO keyspace`, `CONFIG GET maxmemory` via `redis-cli` command builder (`REDISCLI_AUTH` when password set).
  - Parsers with fixtures under `src-tauri/tests/fixtures/redis/` and unit tests.
  - Derived metrics: ops/s, hit rate, memory used/peak/max ratio, clients, keys per DB, evictions, expirations, replication role.
- `SecretKind::RedisPassword` in keyring (account `server:{id}:redis_password`); deleted with server secrets.
- Tauri commands: `set_redis_password`, `redis_auth_status`, `redis_snapshot_demo` (parser-backed demo payload for UI).

## UI

- Route `/redis` enabled in sidebar; `RedisPage` with stat cards, uPlot charts (throughput, memory, hit rate), keyspace table, slow log table.
- `useRedisDashboard` hook: typed interface; demo snapshot from Rust + mock time series until live SSH polling lands.

## Verified

| Check | Result |
|--------|--------|
| `cargo test` (src-tauri) | **Pass** (7 tests) |
| `cargo check` (src-tauri) | **Pass** (dead-code warnings on collector until SSH wiring) |
| `npm run build` | **Pass** |

## Merge notes

- No changes to SSH session, ConnectionManager, Overview, or Server views.
- Shared touch points: `lib.rs` (handler + `collectors` mod), `commands/mod.rs` (`pub mod redis`), `secrets/mod.rs` (Redis password kind), `App.tsx`, `app-sidebar.tsx`, `app-shell.tsx`, `package.json` (+ `uplot`).
