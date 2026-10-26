import { useEffect, useState } from "react";
import { listenThen } from "@/lib/events";
import {
  getWebTrafficSnapshot,
  subscribeWebTraffic,
  unsubscribeWebTraffic,
} from "@/lib/api";
import { queryMetricSeries } from "@/lib/metrics-api";
import { useTimeRange } from "@/contexts/time-range-context";
import type {
  LiveLogLine,
  WebTrafficData,
  WebTrafficFilters,
  WebTrafficMetrics,
} from "@/lib/web-traffic-types";

const EMPTY_METRICS: WebTrafficMetrics = {
  reqPerSec: 0,
  bandwidthMbps: 0,
  statusBreakdown: {},
  topUrls: [],
  topIps: [],
  reqPerSecSeries: [],
};

interface WebTrafficEvent {
  serverId: string;
  metrics: WebTrafficMetrics;
  logs: LiveLogLine[];
  permissionDenied: boolean;
  webDetected: boolean;
}

export function useWebTraffic(serverId: string | null): WebTrafficData & {
  permissionDenied: boolean;
  webDetected: boolean;
} {
  const { range, isLive } = useTimeRange();
  const [metrics, setMetrics] = useState<WebTrafficMetrics>(EMPTY_METRICS);
  const [logs, setLogs] = useState<LiveLogLine[]>([]);
  const [loading, setLoading] = useState(true);
  const [permissionDenied, setPermissionDenied] = useState(false);
  const [webDetected, setWebDetected] = useState(false);

  useEffect(() => {
    if (!serverId) {
      setMetrics(EMPTY_METRICS);
      setLogs([]);
      setLoading(false);
      return;
    }

    if (!isLive) {
      let cancelled = false;
      setLoading(true);
      void (async () => {
        try {
          const result = await queryMetricSeries({
            serverId,
            metricKey: "web.req_per_sec",
            rangeStartMs: range.startMs,
            rangeEndMs: range.endMs,
            preferLive: false,
          });
          if (cancelled) return;
          const series = result.points.map((p) => ({
            t: p.tsMs,
            v: p.value,
          }));
          const last = series[series.length - 1]?.v ?? 0;
          setMetrics({
            ...EMPTY_METRICS,
            reqPerSec: last,
            reqPerSecSeries: series,
          });
          setLogs([]);
          setPermissionDenied(false);
        } finally {
          if (!cancelled) setLoading(false);
        }
      })();
      return () => {
        cancelled = true;
      };
    }

    setLoading(true);
    const apply = (payload: WebTrafficEvent) => {
      setMetrics(payload.metrics);
      setLogs(payload.logs);
      setPermissionDenied(payload.permissionDenied);
      setWebDetected(payload.webDetected);
      setLoading(false);
    };
    const stop = listenThen<WebTrafficEvent>(
      `web-traffic:${serverId}`,
      apply,
      async () => {
        const cached = await getWebTrafficSnapshot(serverId);
        if (cached) apply(cached);
        await subscribeWebTraffic(serverId);
      },
    );

    return () => {
      stop();
      void unsubscribeWebTraffic(serverId);
    };
  }, [serverId, isLive, range.startMs, range.endMs]);

  return {
    metrics,
    logs,
    loading,
    permissionDenied,
    webDetected,
  };
}

export function filterWebLogs(
  logs: LiveLogLine[],
  filters: WebTrafficFilters,
): LiveLogLine[] {
  return logs.filter((line) => {
    if (filters.statusMin !== undefined && line.status < filters.statusMin) {
      return false;
    }
    if (filters.statusMax !== undefined && line.status > filters.statusMax) {
      return false;
    }
    if (
      filters.pathQuery &&
      !line.path.toLowerCase().includes(filters.pathQuery.toLowerCase())
    ) {
      return false;
    }
    if (
      filters.ipQuery &&
      !line.remoteAddr.includes(filters.ipQuery)
    ) {
      return false;
    }
    if (
      filters.textQuery &&
      !line.raw.toLowerCase().includes(filters.textQuery.toLowerCase())
    ) {
      return false;
    }
    return true;
  });
}
