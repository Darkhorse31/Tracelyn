import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  RedisDashboardData,
  RedisSnapshot,
  RedisTimeSeriesPoint,
} from "@/lib/redis-types";
import { useServersContext } from "@/contexts/servers-context";

const MOCK_SERIES_LEN = 60;

function buildMockSeries(snapshot: RedisSnapshot): RedisTimeSeriesPoint[] {
  const now = Date.now();
  const baseOps = snapshot.derived.opsPerSec;
  const baseMem = snapshot.derived.memoryUsedBytes;
  const baseHit = snapshot.derived.hitRatePercent;
  const points: RedisTimeSeriesPoint[] = [];
  for (let i = MOCK_SERIES_LEN - 1; i >= 0; i--) {
    const phase = (MOCK_SERIES_LEN - i) / MOCK_SERIES_LEN;
    const wobble = Math.sin(phase * Math.PI * 4) * 0.08;
    points.push({
      t: now - i * 5000,
      opsPerSec: Math.max(0, baseOps * (1 + wobble)),
      memoryUsedBytes: Math.round(baseMem * (1 + wobble * 0.02)),
      hitRatePercent: Math.min(100, Math.max(0, baseHit + wobble * 5)),
    });
  }
  return points;
}

async function fetchDemoSnapshot(): Promise<RedisSnapshot> {
  return invoke<RedisSnapshot>("redis_snapshot_demo");
}

async function fetchAuthConfigured(serverId: string): Promise<boolean> {
  const status = await invoke<{ configured: boolean }>("redis_auth_status", {
    serverId,
  });
  return status.configured;
}

export interface UseRedisDashboardResult extends RedisDashboardData {
  refresh: () => void;
  setRedisPassword: (password: string) => Promise<void>;
}

/**
 * Data hook for the Redis view. Uses typed mock series + demo snapshot from Rust parsers
 * until ConnectionManager wires live polling.
 */
export function useRedisDashboard(): UseRedisDashboardResult {
  const { selectedServerId } = useServersContext();
  const [snapshot, setSnapshot] = useState<RedisSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [authConfigured, setAuthConfigured] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const snap = await fetchDemoSnapshot();
      setSnapshot(snap);
      if (selectedServerId) {
        setAuthConfigured(await fetchAuthConfigured(selectedServerId));
      } else {
        setAuthConfigured(false);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [selectedServerId]);

  useEffect(() => {
    void load();
    const id = window.setInterval(() => void load(), 5000);
    return () => window.clearInterval(id);
  }, [load]);

  const series = useMemo(
    () => (snapshot ? buildMockSeries(snapshot) : []),
    [snapshot],
  );

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
    refresh: () => void load(),
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
