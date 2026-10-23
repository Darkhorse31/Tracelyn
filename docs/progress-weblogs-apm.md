# Tracelyn — Web traffic & APM core (parallel branch)

Date: 2026-10-23  
Scope: Phase 1 Steps 3–4 core (no ConnectionManager / system collector / Overview–Server changes).

## Rust (`src-tauri/src/webserver/`)

- **Detection** (`detect.rs`): nginx vs Apache from `ps` and version probe outputs.
- **Config** (`config.rs`): parse access/error log paths, `log_format` / `LogFormat`, detect missing `$request_time` / `%D` with suggested one-line fixes.
- **Parsing** (`parse.rs`): common/combined/custom nginx lines; Apache `%D` microseconds.
- **Routes** (`route.rs`): numeric segments, UUIDs, long hex hashes → `:id`; manual regex rules.
- **Quantiles** (`quantile.rs`): P² streaming estimators for p50/p95/p99.
- **Aggregation** (`aggregate.rs`): req/s, status breakdown, top URLs/IPs, bandwidth, per-route metrics, heatmap buckets, capped live log buffer.
- **`RemoteExec` trait** + `detect_and_parse_config` for later SSH wiring.
- **Tauri helpers** (`commands/webtraffic.rs`): parse line, detect, parse config, normalize route.
- **Tests**: 11 unit tests + fixtures under `src-tauri/tests/fixtures/`.

## UI

- **Web traffic** (`/web`): metrics cards, sparkline, stacked status bar, top URLs/IPs, timing notice, virtualized filterable log (`@tanstack/react-virtual`).
- **API endpoints** (`/apm`): latency bands, heatmap, slowest/most-failing tables, route rules editor, per-endpoint status stacks.
- **Data hooks**: `useWebTraffic`, `useApmEndpoints` (typed mock data; swap when events are wired).

## Verified

| Check | Result |
|--------|--------|
| `cargo test` (src-tauri) | **Pass** (11 tests) |
| `cargo check` (src-tauri) | **Pass** |
| `npm run build` | **Pass** |

## Merge notes

- New module `webserver` only; shared touch points: `lib.rs` (module + 4 commands), `App.tsx`, sidebar, `app-shell` titles, `package.json` (`@tanstack/react-virtual`), `Cargo.toml` (`regex`).
