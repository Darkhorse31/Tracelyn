import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import {
  getRedisSnapshot,
  setRedisTarget,
  setRedisUsername,
  subscribeRedisMetrics,
  unsubscribeRedisMetrics,
  type RedisTargetConfig,
} from "@/lib/api";
import { listenThen } from "@/lib/events";
import { queryMetricSeries } from "@/lib/metrics-api";
import { useTimeRange } from "@/contexts/time-range-context";
import type {
  RedisDashboardData,
  RedisSnapshot,
  RedisTimeSeriesPoint,
} from "@/lib/redis-types";
import { useServersContext } from "@/contexts/servers-context";

async function fetchAuthConfigured(serverId: string): Promise<boolean> {
  const status = await invoke<{ configured: boolean }>("redis_auth_status", {
    serverId,
  });
  return status.configured;
}

export interface UseRedisDashboardResult extends RedisDashboardData {
  refresh: () => void;
  setRedisPassword: (password: string) => Promise<void>;
  saveRedisConnection: (input: {
    username?: string;
    password?: string;
    target?: RedisTargetConfig;
  }) => Promise<void>;
  redisDetected: boolean;
}

export function useRedisDashboard(): UseRedisDashboardResult {
  const { selectedServerId } = useServersContext();
  const { range, isLive } = useTimeRange();
  const [snapshot, setSnapshot] = useState<RedisSnapshot | null>(null);
  const [series, setSeries] = useState<RedisTimeSeriesPoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [authConfigured, setAuthConfigured] = useState(false);
  const [redisDetected, setRedisDetected] = useState(false);

  const loadHistorical = useCallback(async () => {
    if (!selectedServerId) return;
    setLoading(true);
    setError(null);
    try {
      const [ops, mem, hit] = await Promise.all([
        queryMetricSeries({
          serverId: selectedServerId,
          metricKey: "redis.ops_per_sec",
          rangeStartMs: range.startMs,
          rangeEndMs: range.endMs,
          preferLive: false,
        }),
        queryMetricSeries({
          serverId: selectedServerId,
          metricKey: "redis.memory_used_bytes",
          rangeStartMs: range.startMs,
          rangeEndMs: range.endMs,
          preferLive: false,
        }),
        queryMetricSeries({
          serverId: selectedServerId,
          metricKey: "redis.hit_rate_percent",
          rangeStartMs: range.startMs,
          rangeEndMs: range.endMs,
          preferLive: false,
        }),
      ]);
      const points: RedisTimeSeriesPoint[] = ops.points.map((p, i) => ({
        t: p.tsMs,
        opsPerSec: p.value,
        memoryUsedBytes: mem.points[i]?.value ?? 0,
        hitRatePercent: hit.points[i]?.value ?? 0,
      }));
      setSeries(points);
      setAuthConfigured(await fetchAuthConfigured(selectedServerId));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [selectedServerId, range.startMs, range.endMs]);

  useEffect(() => {
    if (!selectedServerId) {
      setSnapshot(null);
      setSeries([]);
      setLoading(false);
      return;
    }

    if (!isLive) {
      void loadHistorical();
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);
    setSeries([]);

    const apply = (snap: RedisSnapshot) => {
      if (cancelled) return;
      setSnapshot(snap);
      setRedisDetected(snap.collectorState?.phase !== "not_installed");
      setLoading(false);
      if (snap.collectorState?.phase === "live" && snap.collectedAtMs > 0) {
        setSeries((prev) => {
          const point: RedisTimeSeriesPoint = {
            t: snap.collectedAtMs,
            opsPerSec: snap.derived.opsPerSec,
            memoryUsedBytes: snap.derived.memoryUsedBytes,
            hitRatePercent: snap.derived.hitRatePercent,
          };
          const next = [...prev, point];
          return next.length > 60 ? next.slice(-60) : next;
        });
      }
    };

    void fetchAuthConfigured(selectedServerId).then(
      (configured) => !cancelled && setAuthConfigured(configured),
      (err) => !cancelled && setError(String(err)),
    );
    const stop = listenThen<RedisSnapshot>(
      `redis-metrics:${selectedServerId}`,
      apply,
      async () => {
        const cached = await getRedisSnapshot(selectedServerId);
        if (cached && !cancelled) apply(cached as RedisSnapshot);
        await subscribeRedisMetrics(selectedServerId);
      },
    );

    return () => {
      cancelled = true;
      stop();
      void unsubscribeRedisMetrics(selectedServerId);
    };
  }, [selectedServerId, isLive, loadHistorical]);

  const setRedisPassword = useCallback(
    async (password: string) => {
      if (!selectedServerId) {
        throw new Error("Select a server in the sidebar first");
      }
      await invoke("set_redis_password", {
        serverId: selectedServerId,
        password,
      });
      setAuthConfigured(true);
    },
    [selectedServerId],
  );

  const saveRedisConnection = useCallback(
    async (input: {
      username?: string;
      password?: string;
      target?: RedisTargetConfig;
    }) => {
      if (!selectedServerId) {
        throw new Error("Select a server in the sidebar first");
      }
      if (input.password) {
        await invoke("set_redis_password", {
          serverId: selectedServerId,
          password: input.password,
        });
        setAuthConfigured(true);
      }
      if (input.username) {
        await setRedisUsername(selectedServerId, input.username);
      }
      if (input.target) {
        await setRedisTarget(selectedServerId, input.target);
      }
    },
    [selectedServerId],
  );

  return {
    snapshot: snapshot ?? emptySnapshot(),
    series,
    loading,
    error,
    authConfigured,
    redisDetected,
    refresh: () => void loadHistorical(),
    setRedisPassword,
    saveRedisConnection,
  };
}

function emptySnapshot(): RedisSnapshot {
  return {
    collectedAtMs: 0,
    info: {},
    keyspace: [],
    slowlog: [],
    derived: {
      opsPerSec: 0,
      hitRatePercent: 0,
      memoryUsedBytes: 0,
      memoryPeakBytes: 0,
      maxmemoryBytes: 0,
      connectedClients: 0,
      evictedKeys: 0,
      expiredKeys: 0,
      role: "—",
      totalKeys: 0,
    },
  };
}
