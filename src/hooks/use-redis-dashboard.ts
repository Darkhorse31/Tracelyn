import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import {
  subscribeRedisMetrics,
  unsubscribeRedisMetrics,
} from "@/lib/api";
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

    let unlisten: UnlistenFn | undefined;
    setLoading(true);
    setError(null);
    setSeries([]);

    void (async () => {
      await subscribeRedisMetrics(selectedServerId);
      setAuthConfigured(await fetchAuthConfigured(selectedServerId));
      unlisten = await listen<RedisSnapshot>(
        `redis-metrics:${selectedServerId}`,
        (event) => {
          const snap = event.payload;
          setSnapshot(snap);
          setRedisDetected(true);
          setLoading(false);
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
        },
      );
    })();

    return () => {
      void unlisten?.();
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

  return {
    snapshot: snapshot ?? emptySnapshot(),
    series,
    loading,
    error,
    authConfigured,
    redisDetected,
    refresh: () => void loadHistorical(),
    setRedisPassword,
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
