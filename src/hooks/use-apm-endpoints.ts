import { useMemo, useState } from "react";
import type { ApmData, EndpointRow, RouteRule } from "@/lib/apm-types";

const ENDPOINTS: EndpointRow[] = [
  {
    route: "/api/orders/:id/items",
    throughput: 18.2,
    avgMs: 84,
    p50Ms: 62,
    p95Ms: 210,
    p99Ms: 420,
    errorRate: 0.02,
    statusBreakdown: { "200": 1800, "500": 36 },
  },
  {
    route: "/api/users/:id",
    throughput: 12.4,
    avgMs: 45,
    p50Ms: 38,
    p95Ms: 95,
    p99Ms: 180,
    errorRate: 0.004,
    statusBreakdown: { "200": 2400, "404": 12 },
  },
  {
    route: "/checkout",
    throughput: 4.1,
    avgMs: 320,
    p50Ms: 280,
    p95Ms: 890,
    p99Ms: 1200,
    errorRate: 0.08,
    statusBreakdown: { "200": 380, "500": 32, "302": 44 },
  },
];

function heatmapFromEndpoints(rows: EndpointRow[]) {
  const cells = [];
  for (const row of rows) {
    for (let b = 0; b < 24; b++) {
      cells.push({
        route: row.route,
        bucketIndex: b,
        p50Ms: row.p50Ms * (0.85 + (b % 5) * 0.05),
        p95Ms: row.p95Ms * (0.9 + (b % 4) * 0.06),
      });
    }
  }
  return cells;
}

/** Data hook — swap when Rust aggregator events are subscribed. */
export function useApmEndpoints(_serverId: string | null): ApmData & {
  addRouteRule: (rule: RouteRule) => void;
} {
  const [rules, setRules] = useState<RouteRule[]>([
    { pattern: "^/api/v\\d+", replacement: "/api/v:ver" },
  ]);

  const endpoints = useMemo(() => ENDPOINTS, []);
  const slowest = useMemo(
    () => [...endpoints].sort((a, b) => b.p95Ms - a.p95Ms).slice(0, 5),
    [endpoints],
  );
  const mostFailing = useMemo(
    () => [...endpoints].sort((a, b) => b.errorRate - a.errorRate).slice(0, 5),
    [endpoints],
  );

  const now = Date.now();
  const latencySeries = endpoints.map((ep) => ({
    route: ep.route,
    points: Array.from({ length: 40 }, (_, i) => ({
      t: now - (39 - i) * 15000,
      p50: ep.p50Ms * (0.9 + Math.sin(i / 6) * 0.1),
      p95: ep.p95Ms * (0.9 + Math.sin(i / 5) * 0.12),
      p99: ep.p99Ms * (0.9 + Math.sin(i / 4) * 0.15),
    })),
  }));

  return {
    endpoints,
    heatmap: heatmapFromEndpoints(endpoints),
    slowest,
    mostFailing,
    routeRules: rules,
    latencySeries,
    loading: false,
    addRouteRule: (rule) => setRules((r) => [...r, rule]),
  };
}
