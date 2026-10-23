# Tracelyn — Progress (feat/storage-alerts)

Parallel work: Steps 6 & 7 core. Does **not** modify ConnectionManager, system collector, or Overview/Server pages.

## Rust — `storage/`

- SQLite (`metrics.db` under `%LOCALAPPDATA%\Tracelyn\`) via **rusqlite** (bundled).
- **Raw** samples table + **1-minute rollups**; retention defaults **7d raw / 90d rollups** (stored in `app_settings`, Tauri `get_retention_config` / `set_retention_config`).
- Jobs: `rollup_raw_to_minute`, `prune_samples`, `run_storage_jobs` command.
- Query: `query_metric_series` with automatic raw vs rollup resolution (`choose_resolution`).
- Unit tests: rollup aggregation, pruning, resolution selection.

## Rust — `alerts/`

- Default rules seeded once (spec §8): unreachable 60s, CPU/memory 90% for 5m, disk 85%, 5xx >5% for 2m, endpoint p95 (500ms default), Redis memory 90%.
- Rule engine with duration gates + clear-below hysteresis (30s clear hold).
- Alert history in SQLite; Windows notifications on fire via **tauri-plugin-notification**.
- Tauri commands: rules CRUD, history, `insert_metric_samples`, `evaluate_alerts`.
- Unit tests: CPU duration, disk hysteresis clear.

## Rust — `metrics_ingest.rs`

- `RemoteCommandRunner` / `RemoteStreamReader` / `MetricCollector` trait for later ConnectionManager wiring.

## UI

- **Alerts** page (`/alerts`): rule list + inline editor, history panel (mock data via `useAlerts`; flip `USE_TAURI_ALERTS` in hook when wiring).
- **`TimeRangePicker`** component (`live`, 15m, 1h, 24h, 7d, custom) — not wired to top bar yet.
- Sidebar enables Alerts route; top bar time range placeholder unchanged.

## Verified

| Check | Result |
|--------|--------|
| `cargo test` (src-tauri) | **Pass** (6 tests) |
| `cargo check` (src-tauri) | **Pass** |
| `npm run build` | **Pass** |

## Merge notes

- Shared touchpoints: `lib.rs` (modules + commands + notification plugin), `state/mod.rs` (`metrics` store), `commands/mod.rs` (`storage_alerts` submodule), `capabilities/default.json`, `Cargo.toml`.
- Wire collectors → `insert_metric_samples` + `evaluate_alerts`; set `USE_TAURI_ALERTS = true` in `use-alerts.ts`; replace top-bar placeholder with `TimeRangePicker` when views consume history.
