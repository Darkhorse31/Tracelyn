import { useCallback, useEffect, useMemo, useState } from "react";
import { listenThen } from "@/lib/events";
import {
  getApmSnapshot,
  getRouteRules,
  setRouteRules,
  subscribeApmMetrics,
  unsubscribeApmMetrics,
} from "@/lib/api";
import { useTimeRange } from "@/contexts/time-range-context";
import type { ApmData, EndpointRow, RouteRule } from "@/lib/apm-types";

interface ApmMetricsEvent {
  serverId: string;
  endpoints: EndpointRow[];
  heatmap: ApmData["heatmap"];
  timingNotice?: { message: string; suggestedLine: string };
}

type LatencyPoint = ApmData["latencySeries"][number]["points"][number];

/** About two minutes of history at one APM update per second. */
const HISTORY_LEN = 120;

export function useApmEndpoints(serverId: string | null): ApmData & {
  saveRouteRules: (rules: RouteRule[]) => Promise<void>;
} {
  const { isLive } = useTimeRange();
  const [endpoints, setEndpoints] = useState<EndpointRow[]>([]);
  const [heatmap, setHeatmap] = useState<ApmData["heatmap"]>([]);
  const [history, setHistory] = useState<Map<string, LatencyPoint[]>>(() => new Map());
  const [timingNotice, setTimingNotice] = useState<ApmData["timingNotice"]>();
  const [loading, setLoading] = useState(true);
  const [rules, setRules] = useState<RouteRule[]>([]);

  useEffect(() => {
    setRules([]);
    if (!serverId) return;
    let cancelled = false;
    void getRouteRules(serverId).then((r) => {
      if (!cancelled) setRules(r);
    });
    return () => {
      cancelled = true;
    };
  }, [serverId]);

  useEffect(() => {
    setEndpoints([]);
    setHeatmap([]);
    setHistory(new Map());
    if (!serverId || !isLive) {
      setLoading(false);
      return;
    }

    setLoading(true);
    const apply = (payload: ApmMetricsEvent) => {
      const t = Date.now();
      setEndpoints(payload.endpoints);
      setHeatmap(payload.heatmap);
      setTimingNotice(payload.timingNotice);
      setHistory((prev) => {
        const next = new Map<string, LatencyPoint[]>();
        for (const ep of payload.endpoints) {
          const points = [
            ...(prev.get(ep.route) ?? []),
            { t, p50: ep.p50Ms, p95: ep.p95Ms, p99: ep.p99Ms },
          ];
          next.set(ep.route, points.slice(-HISTORY_LEN));
        }
        return next;
      });
      setLoading(false);
    };
    const stop = listenThen<ApmMetricsEvent>(
      `apm-metrics:${serverId}`,
      apply,
      async () => {
        const cached = await getApmSnapshot(serverId);
        if (cached) apply(cached);
        await subscribeApmMetrics(serverId);
      },
    );

    return () => {
      stop();
      void unsubscribeApmMetrics(serverId);
    };
  }, [serverId, isLive]);

  const saveRouteRules = useCallback(
    async (next: RouteRule[]) => {
      if (!serverId) return;
      setRules(await setRouteRules(serverId, next));
    },
    [serverId],
  );

  const slowest = useMemo(
    () => [...endpoints].sort((a, b) => b.p95Ms - a.p95Ms).slice(0, 8),
    [endpoints],
  );
  const mostFailing = useMemo(
    () =>
      [...endpoints]
        .filter((e) => e.errorRate > 0)
        .sort((a, b) => b.errorRate - a.errorRate)
        .slice(0, 8),
    [endpoints],
  );
  const latencySeries = useMemo(
    () => [...history].map(([route, points]) => ({ route, points })),
    [history],
  );

  return {
    endpoints,
    heatmap,
    slowest,
    mostFailing,
    routeRules: rules,
    latencySeries,
    timingNotice,
    loading,
    saveRouteRules,
  };
}
