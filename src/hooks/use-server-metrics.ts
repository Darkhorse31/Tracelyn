import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import {
  subscribeServerMetrics,
  unsubscribeServerMetrics,
} from "@/lib/api";
import { queryMetricSeries } from "@/lib/metrics-api";
import { useTimeRange } from "@/contexts/time-range-context";
import type { MetricPoint, ServerMetricsSnapshot } from "@/lib/types";

export function useServerMetrics(serverId: string | null, enabled: boolean) {
  const { range, isLive } = useTimeRange();
  const [snapshot, setSnapshot] = useState<ServerMetricsSnapshot | null>(null);
  const [waiting, setWaiting] = useState(false);

  useEffect(() => {
    if (!enabled || !serverId) {
      setSnapshot(null);
      setWaiting(false);
      return;
    }

    if (!isLive) {
      let cancelled = false;
      setWaiting(true);
      void (async () => {
        try {
          const cpu = await queryMetricSeries({
            serverId,
            metricKey: "cpu.percent",
            rangeStartMs: range.startMs,
            rangeEndMs: range.endMs,
            preferLive: false,
          });
          if (cancelled) return;
          const history: MetricPoint[] = cpu.points.map((p) => ({
            t: p.tsMs,
            v: p.value,
          }));
          const last = history[history.length - 1]?.v ?? 0;
          setSnapshot({
            serverId,
            timestampMs: Date.now(),
            cpuPercent: last,
            memoryPercent: 0,
            load1: 0,
            load5: 0,
            load15: 0,
            diskPercent: 0,
            diskReadBps: 0,
            diskWriteBps: 0,
            netInBps: 0,
            netOutBps: 0,
            cpuHistory: history,
            memoryHistory: [],
            loadHistory: [],
            diskUsageHistory: [],
            diskReadHistory: [],
            diskWriteHistory: [],
            netInHistory: [],
            netOutHistory: [],
            processes: [],
            services: [],
            ssSummary: {},
            collectorError: null,
          });
        } finally {
          if (!cancelled) setWaiting(false);
        }
      })();
      return () => {
        cancelled = true;
      };
    }

    let unlisten: UnlistenFn | undefined;
    setWaiting(true);

    void (async () => {
      await subscribeServerMetrics(serverId);
      const eventName = `server-metrics:${serverId}`;
      unlisten = await listen<ServerMetricsSnapshot>(eventName, (event) => {
        setSnapshot(event.payload);
        setWaiting(false);
      });
    })();

    return () => {
      void unlisten?.();
      void unsubscribeServerMetrics(serverId);
    };
  }, [serverId, enabled, isLive, range.startMs, range.endMs]);

  return { snapshot, waiting };
}
