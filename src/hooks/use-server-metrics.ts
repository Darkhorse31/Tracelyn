import { useEffect, useState } from "react";
import { listenWhileMounted } from "@/lib/events";
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
      const load = async (metricKey: string): Promise<MetricPoint[]> => {
        const res = await queryMetricSeries({
          serverId,
          metricKey,
          rangeStartMs: range.startMs,
          rangeEndMs: range.endMs,
          preferLive: false,
        });
        return res.points.map((p) => ({ t: p.tsMs, v: p.value }));
      };
      const last = (points: MetricPoint[]) => points[points.length - 1]?.v ?? 0;
      void (async () => {
        try {
          const [cpu, memory, disk, load1, diskRead, diskWrite, netIn, netOut] = await Promise.all(
            ["cpu.percent", "memory.percent", "disk.percent", "load.1", "disk.read_bps", "disk.write_bps", "net.rx_bps", "net.tx_bps"].map(load),
          );
          if (cancelled) return;
          setSnapshot({
            serverId,
            timestampMs: Date.now(),
            cpuPercent: last(cpu),
            memoryPercent: last(memory),
            load1: last(load1),
            load5: 0,
            load15: 0,
            diskPercent: last(disk),
            diskReadBps: last(diskRead),
            diskWriteBps: last(diskWrite),
            netInBps: last(netIn),
            netOutBps: last(netOut),
            cpuHistory: cpu,
            memoryHistory: memory,
            loadHistory: load1,
            diskUsageHistory: disk,
            diskReadHistory: diskRead,
            diskWriteHistory: diskWrite,
            netInHistory: netIn,
            netOutHistory: netOut,
            processes: [],
            services: [],
            ssSummary: {},
            collectorError: null,
            collectorName: null,
            collectorFailures: 0,
          });
        } catch {
          if (!cancelled) setSnapshot(null);
        } finally {
          if (!cancelled) setWaiting(false);
        }
      })();
      return () => {
        cancelled = true;
      };
    }

    setWaiting(true);
    void subscribeServerMetrics(serverId);
    const stop = listenWhileMounted<ServerMetricsSnapshot>(
      `server-metrics:${serverId}`,
      (event) => {
        setSnapshot(event.payload);
        setWaiting(false);
      },
    );

    return () => {
      stop();
      void unsubscribeServerMetrics(serverId);
    };
  }, [serverId, enabled, isLive, range.startMs, range.endMs]);

  return { snapshot, waiting };
}
