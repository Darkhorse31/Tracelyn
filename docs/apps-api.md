# Applications API — Tracelyn backend contract

This document defines the Rust/Tauri contract for **application discovery**, **manual registry**, **per-app logs**, and **diagnostics**. All payloads use `camelCase` in JSON.

## App model

```ts
type AppKind =
  | "pm2"
  | "systemd"
  | "docker"
  | "nginx"
  | "apache"
  | "service"
  | "manual";

type AppOrigin = "discovered" | "manual";

type AppStatus = "online" | "stopped" | "errored" | "unknown";

interface LogSourceRef {
  id: string;
  kind:
    | "stdout"
    | "stderr"
    | "journald"
    | "docker"
    | "nginx_access"
    | "nginx_error"
    | "apache_access"
    | "custom";
  path?: string;        // file path or journal unit
  containerId?: string;
  vhost?: string;
  readable: boolean;
  issue?: string;       // permission_denied | missing | rotated
  fixHint?: string;
}

interface TracelynApp {
  id: string;           // stable: hash(serverId + kind + name) or manual uuid
  serverId: string;
  name: string;
  kind: AppKind;
  status: AppStatus;
  origin: AppOrigin;
  hidden: boolean;
  pids: number[];
  ports: number[];
  domains: string[];
  tags: string[];
  logSources: LogSourceRef[];
  cpuPercent?: number;
  memoryBytes?: number;
  restarts?: number;
  uptimeSecs?: number;
  cwd?: string;
  interpreter?: string;
  linkedAppIds?: string[]; // merge duplicates
  metrics?: {
    reqPerSec?: number;
    errorRate?: number;
    p95Ms?: number;
  };
}
```

## Discovery (SSH, read-only)

Discovery runs on the connection loop every **60s** and on `rediscover_apps`. Commands use the existing SSH allowlist; when the login user is not root, probes use `sudo -n` (no password prompt).

| Source | Remote command | Parser module |
|--------|----------------|---------------|
| PM2 | `pm2 jlist` + per-user `PM2_HOME=… pm2 jlist` | `apps::parsers::pm2` |
| systemd | `systemctl list-units --type=service --state=running` + unit files under `/etc/systemd/system` | `apps::parsers::systemd` |
| Docker | `docker ps --format '{{json .}}'` (line-delimited JSON) | `apps::parsers::docker` |
| Nginx | `nginx -T` server blocks | `apps::parsers::nginx_vhost` |
| Apache | `apache2ctl -S` / `httpd -S` | `apps::parsers::apache_vhost` |
| Listeners | `ss -ltnpH` | `apps::parsers::ss_listen` |

**Merge pipeline** (`apps::merge`):

1. Build apps from PM2, systemd, Docker.
2. Attach nginx/apache vhosts (domains + access/error log paths).
3. Map `proxy_pass` / upstream ports → `ss` listeners → owning app PID.
4. Emit unmatched listeners as `kind: "service"` (MySQL, Postgres, Redis, etc.).
5. Overlay manual rows from SQLite (same `id` wins manual fields; `hidden` respected).
6. Deduplicate by normalized name + overlapping ports (suggest merge in UI).

## SQLite (`app_registry`)

Table `app_registry` in the metrics DB (`%LOCALAPPDATA%\Tracelyn\metrics.db`):

| Column | Type | Notes |
|--------|------|-------|
| server_id | TEXT | FK logical |
| app_id | TEXT | PK with server_id |
| json | TEXT | full `TracelynApp` manual overrides |
| updated_at_ms | INTEGER | |

Manual CRUD does not delete discovered rows; it stores overrides keyed by `app_id`.

## Tauri commands

| Command | Args | Returns |
|---------|------|---------|
| `list_apps` | `{ serverId }` | `TracelynApp[]` |
| `get_app` | `{ serverId, appId }` | `TracelynApp \| null` |
| `upsert_manual_app` | `{ serverId, app: TracelynApp }` | `TracelynApp` |
| `hide_app` | `{ serverId, appId, hidden: boolean }` | `void` |
| `merge_apps` | `{ serverId, primaryId, duplicateIds: string[] }` | `TracelynApp` |
| `rediscover_apps` | `{ serverId }` | `TracelynApp[]` |
| `subscribe_apps` | `{ serverId }` | `void` |
| `unsubscribe_apps` | `{ serverId }` | `void` |

Events:

- `apps-updated:{serverId}` — full list after discovery or manual edit.
- `app-metrics:{serverId}:{appId}` — optional live CPU/mem/req metrics (batched).

## Log explorer API

| Command | Args | Returns |
|---------|------|---------|
| `subscribe_log_stream` | `{ serverId, appIds?: string[], sources?: string[] }` | `void` |
| `unsubscribe_log_stream` | `{ serverId }` | `void` |
| `get_log_snapshot` | `{ serverId }` | `LogStreamSnapshot` |

Event: `log-stream:{serverId}`

`lines` on the event are an **incremental batch** (at most 2000 new lines). `incremental: false` on `get_log_snapshot` is the recent window (last 2000). The Rust ring still holds 100_000 lines. The UI appends batches and virtualizes up to 100_000.

`subscribe_log_stream` accepts optional `appIds` and `sources` to limit which files are tailed (max 8 channels). Nginx and Apache access logs are tailed by the web pipeline (up to 24 vhost files) and copied into the same stream, tagged with the owning app.

```ts
interface LogLine {
  id: string;
  tsMs: number;
  appId?: string;
  sourceId: string;
  level: "trace" | "debug" | "info" | "warn" | "error" | "fatal" | "unknown";
  message: string;
  raw: string;
  httpStatus?: number;
  multilineGroupId?: string;
}

interface LogStreamSnapshot {
  lines: LogLine[];
  incremental: boolean;
  paused: boolean;
  sourceStatus: Record<string, { ok: boolean; reason?: string; fixHint?: string }>;
  facets: { level: Record<string, number>; appId: Record<string, number> };
}
```

Rust tails with `tail -n 500 -F` (one SSH channel per source, max **8** concurrent tails per server). Ring buffer **100_000** lines per server; UI virtualizes via existing `VirtualLogList`.

## Diagnostics API

| Command | Args | Returns |
|---------|------|---------|
| `get_server_diagnostics` | `{ serverId }` | `DiagnosticEntry[]` |

```ts
interface DiagnosticEntry {
  collector: string; // system | pm2 | systemd | docker | nginx | apache | redis | logs | apps
  status: "ok" | "warning" | "error";
  message: string;
  fixHint?: string;
}
```

Updated on each discovery pass and collector tick; cached in `AppState` per server.

## Redis collector states (UI)

`redis-metrics:{serverId}` payload adds:

```ts
interface RedisCollectorState {
  phase:
    | "not_installed"
    | "auth_required"
    | "connection_refused"
    | "timed_out"
    | "loading"
    | "live";
  message?: string;
}
```

Password remains in Windows Credential Manager (`set_redis_password`). ACL username is `set_redis_username`. Port, unix socket, and Docker container name are `set_redis_target` / `get_redis_target` (stored in `config.json`, not the keyring).

`redis-cli` is invoked as `timeout 5`, with `-p`, `-s`, `--user`, and `docker exec -e REDISCLI_AUTH` when those are set. Failed polls back off up to 60s. The payload includes `instances[]` when more than one Redis is detected.

`get_redis_snapshot`, `get_web_traffic_snapshot`, and `get_apm_snapshot` return the last cached event so the UI can paint before the next live tick. `subscribe_*` replays that cache only after the frontend listener is registered.

## Web / APM multi-vhost

`WebPipeline` tails **every distinct** `access_log` path from nginx/apache config (deduped). Each line is tagged with `vhostKey` internally for per-app filtering on the App detail **Requests** tab.

## Event hub replay

`EventHub` keeps the latest `web-traffic`, `apm-metrics`, `redis-metrics`, and `apps-updated` payload per `serverId` even when unsubscribed. `subscribe_*` immediately emits the cached snapshot (fixes empty first paint / StrictMode double-mount races).
