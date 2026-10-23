import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import {
  subscribeApmMetrics,
  unsubscribeApmMetrics,
} from "@/lib/api";
import { useTimeRange } from "@/contexts/time-range-context";
import type { ApmData, EndpointRow, RouteRule } from "@/lib/apm-types";

interface ApmMetricsEvent {
  serverId: string;
  endpoints: EndpointRow[];
  heatmap: {
    route: string;
    bucketIndex: number;
    p50Ms: number;
    p95Ms: number;
  }[];
  timingNotice?: { message: string; suggestedLine: string };
}

export function useApmEndpoints(serverId: string | null): ApmData & {
  addRouteRule: (rule: RouteRule) => void;
} {
  const { isLive } = useTimeRange();
  const [endpoints, setEndpoints] = useState<EndpointRow[]>([]);
  const [heatmap, setHeatmap] = useState<ApmMetricsEvent["heatmap"]>([]);
  const [timingNotice, setTimingNotice] = useState<ApmData["timingNotice"]>();
  const [loading, setLoading] = useState(true);
  const [rules, setRules] = useState<RouteRule[]>([
    { pattern: "^/api/v\\d+", replacement: "/api/v:ver" },
  ]);

  useEffect(() => {
    if (!serverId || !isLive) {
      setEndpoints([]);
      setHeatmap([]);
      setLoading(!serverId);
      return;
    }

    let unlisten: UnlistenFn | undefined;
    setLoading(true);

    void (async () => {
      await subscribeApmMetrics(serverId);
      unlisten = await listen<ApmMetricsEvent>(
        `apm-metrics:${serverId}`,
        (event) => {
          const payload = event.payload;
          setEndpoints(payload.endpoints);
          setHeatmap(payload.heatmap);
          setTimingNotice(payload.timingNotice);
          setLoading(false);
        },
      );
    })();

    return () => {
      void unlisten?.();
      void unsubscribeApmMetrics(serverId);
    };
  }, [serverId, isLive]);

  const slowest = useMemo(
    () => [...endpoints].sort((a, b) => b.p95Ms - a.p95Ms).slice(0, 8),
    [endpoints],
  );
  const mostFailing = useMemo(
    () => [...endpoints].sort((a, b) => b.errorRate - a.errorRate).slice(0, 8),
    [endpoints],
  );

  return {
    endpoints,
    heatmap,
    slowest,
    mostFailing,
    routeRules: rules,
    latencySeries: [],
    timingNotice,
    loading,
    addRouteRule: (rule) => setRules((prev) => [...prev, rule]),
  };
}
