import { useState } from "react";
import { LatencyBandChart } from "@/components/apm/latency-band-chart";
import { LatencyHeatmap } from "@/components/apm/latency-heatmap";
import { StatusStackBar } from "@/components/web/status-stack-bar";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useServersContext } from "@/contexts/servers-context";
import { useApmEndpoints } from "@/hooks/use-apm-endpoints";

export function ApmEndpointsPage() {
  const { selectedServerId } = useServersContext();
  const {
    endpoints,
    heatmap,
    slowest,
    mostFailing,
    latencySeries,
    routeRules,
    addRouteRule,
    loading,
  } = useApmEndpoints(selectedServerId);
  const [pattern, setPattern] = useState("");
  const [replacement, setReplacement] = useState("");

  const routes = endpoints.map((e) => e.route);
  const primarySeries = latencySeries[0];

  if (!selectedServerId) {
    return (
      <div className="rounded-xl border border-dashed border-border bg-card/40 px-6 py-12 text-center text-sm text-muted-foreground">
        Select a server to preview API endpoint metrics (mock data).
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-2">
          <CardHeader>
            <CardTitle className="text-sm">
              Latency bands — {primarySeries?.route ?? "—"}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {loading || !primarySeries ? (
              <Skeleton className="h-40 w-full" />
            ) : (
              <LatencyBandChart points={primarySeries.points} />
            )}
            <p className="mt-2 text-xs text-muted-foreground">
              Shaded bands: p50–p95 and p95–p99. Line: p50.
            </p>
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Route rules</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-2">
            <ul className="mb-2 space-y-1 font-mono text-xs text-muted-foreground">
              {routeRules.map((r, i) => (
                <li key={i}>
                  {r.pattern} → {r.replacement}
                </li>
              ))}
            </ul>
            <Input
              placeholder="Regex pattern"
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              className="font-mono text-xs"
            />
            <Input
              placeholder="Replacement"
              value={replacement}
              onChange={(e) => setReplacement(e.target.value)}
              className="font-mono text-xs"
            />
            <Button
              size="sm"
              variant="secondary"
              disabled={!pattern || !replacement}
              onClick={() => {
                addRouteRule({ pattern, replacement });
                setPattern("");
                setReplacement("");
              }}
            >
              Add rule
            </Button>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Latency heatmap (p95)</CardTitle>
        </CardHeader>
        <CardContent>
          {loading ? (
            <Skeleton className="h-32 w-full" />
          ) : (
            <LatencyHeatmap cells={heatmap} routes={routes} />
          )}
        </CardContent>
      </Card>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Slowest endpoints (p95)</CardTitle>
          </CardHeader>
          <CardContent>
            <EndpointTable rows={slowest} />
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Most failing</CardTitle>
          </CardHeader>
          <CardContent>
            <EndpointTable rows={mostFailing} showErrorRate />
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">All endpoints</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          {endpoints.map((ep) => (
            <div
              key={ep.route}
              className="rounded-lg border border-border/80 bg-muted/20 p-3"
            >
              <div className="mb-2 flex flex-wrap items-baseline justify-between gap-2">
                <span className="font-mono text-sm">{ep.route}</span>
                <span className="font-mono text-xs tabular-nums text-muted-foreground">
                  {ep.throughput.toFixed(1)} req/s · err{" "}
                  {(ep.errorRate * 100).toFixed(2)}%
                </span>
              </div>
              <div className="mb-2 grid grid-cols-4 gap-2 font-mono text-xs tabular-nums">
                <span>avg {ep.avgMs.toFixed(0)}ms</span>
                <span>p50 {ep.p50Ms.toFixed(0)}</span>
                <span>p95 {ep.p95Ms.toFixed(0)}</span>
                <span>p99 {ep.p99Ms.toFixed(0)}</span>
              </div>
              <StatusStackBar
                breakdown={Object.fromEntries(
                  Object.entries(ep.statusBreakdown).map(([k, v]) => [
                    k.startsWith("2") ? "2xx" : k.startsWith("5") ? "5xx" : `${k}`,
                    v,
                  ]),
                )}
              />
            </div>
          ))}
        </CardContent>
      </Card>
    </div>
  );
}

function EndpointTable({
  rows,
  showErrorRate,
}: {
  rows: {
    route: string;
    p95Ms: number;
    errorRate: number;
    throughput: number;
  }[];
  showErrorRate?: boolean;
}) {
  return (
    <table className="w-full text-left text-xs">
      <thead className="text-muted-foreground">
        <tr>
          <th className="pb-2 font-medium">Route</th>
          <th className="pb-2 font-medium">p95</th>
          {showErrorRate ? (
            <th className="pb-2 font-medium">Error %</th>
          ) : (
            <th className="pb-2 font-medium">req/s</th>
          )}
        </tr>
      </thead>
      <tbody className="font-mono">
        {rows.map((r) => (
          <tr key={r.route} className="border-t border-border/60">
            <td className="py-2 pr-2 truncate max-w-[200px]">{r.route}</td>
            <td className="py-2 tabular-nums">{r.p95Ms.toFixed(0)} ms</td>
            <td className="py-2 tabular-nums">
              {showErrorRate
                ? `${(r.errorRate * 100).toFixed(2)}%`
                : r.throughput.toFixed(1)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
