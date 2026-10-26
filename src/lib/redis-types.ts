export interface RedisDbKeyspace {
  db: number;
  keys: number;
  expires: number;
  avgTtlMs: number;
}

export interface RedisSlowlogEntry {
  id: number;
  timestampUnix: number;
  durationMicros: number;
  command: string;
  client: string;
}

export interface RedisInfoPayload {
  redisVersion?: string | null;
  uptimeSeconds?: number | null;
  role?: string | null;
}

export interface DerivedRedisMetrics {
  opsPerSec: number;
  hitRatePercent: number;
  memoryUsedBytes: number;
  memoryPeakBytes: number;
  maxmemoryBytes: number;
  memoryUsedRatio?: number | null;
  connectedClients: number;
  evictedKeys: number;
  expiredKeys: number;
  role: string;
  totalKeys: number;
}

export type RedisPhase =
  | "not_installed"
  | "auth_required"
  | "connection_refused"
  | "timed_out"
  | "loading"
  | "live";

export interface RedisCollectorState {
  phase: RedisPhase | string;
  message?: string | null;
}

export interface RedisInstanceRef {
  label: string;
  port?: number | null;
  socket?: string | null;
  dockerContainer?: string | null;
}

export interface RedisSnapshot {
  collectedAtMs: number;
  info: RedisInfoPayload;
  keyspace: RedisDbKeyspace[];
  slowlog: RedisSlowlogEntry[];
  derived: DerivedRedisMetrics;
  collectorState?: RedisCollectorState | null;
  instances?: RedisInstanceRef[];
}

/** Time-series point for charts (mock / live). */
export interface RedisTimeSeriesPoint {
  t: number;
  opsPerSec: number;
  memoryUsedBytes: number;
  hitRatePercent: number;
}

export interface RedisDashboardData {
  snapshot: RedisSnapshot;
  series: RedisTimeSeriesPoint[];
  loading: boolean;
  error?: string | null;
  authConfigured: boolean;
}
