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

export interface RedisSnapshot {
  collectedAtMs: number;
  info: RedisInfoPayload;
  keyspace: RedisDbKeyspace[];
  slowlog: RedisSlowlogEntry[];
  derived: DerivedRedisMetrics;
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
