import { useId, useState } from "react";
import { useRememberedState } from "@/hooks/use-remembered-state";
import { Link } from "react-router-dom";
import { Plus, Trash2 } from "lucide-react";
import { formatMs, LatencyBandChart } from "@/components/apm/latency-band-chart";
import { LatencyHeatmap } from "@/components/apm/latency-heatmap";
import { HowToFix } from "@/components/setup/how-to-fix";
import { PageHeader } from "@/components/layout/page-header";
import { EmptyState, Notice } from "@/components/status/notice";
import { StatusStackBar } from "@/components/web/status-stack-bar";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useServersContext } from "@/contexts/servers-context";
import { useTimeRange } from "@/contexts/time-range-context";
import { useApmEndpoints } from "@/hooks/use-apm-endpoints";
import type { EndpointRow, RouteRule } from "@/lib/apm-types";
import { cn } from "cn";

const formatRate = (v: number) => `${v.toFixed(1)} req/s`;
const formatErr = (v: number) => `${(v * 100).toFixed(2)}%`;

function familyBreakdown(statusBreakdown: Record<string, number>) {
  const out: Record<string, number> = {};
  for (const [code, count] of Object.entries(statusBreakdown)) {
    const family = `${code[0]}xx`;
    out[family] = (out[family] ?? 0) + count;
  }
  return out;
}

function RouteRulesCard({
  rules,
  onSave,
}: {
  rules: RouteRule[];
  onSave: (rules: RouteRule[]) => Promise<void>;
}) {
  const id = useId();
  const [pattern, setPattern] = useState("");
  const [replacement, setReplacement] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const save = async (next: RouteRule[]) => {
    setSaving(true);
    setError(null);
    try {
      await onSave(next);
      return true;
    } catch (err) {
      setError(String(err));
      return false;
    } finally {
      setSaving(false);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">Route rules</CardTitle>
        <CardDescription className="text-xs">
          Regex rewrites applied before automatic <span className="font-mono">:id</span>{" "}
          collapsing. New rules affect requests from now on.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col gap-4">
        {rules.length === 0 ? (
          <p className="text-xs text-muted-foreground">No manual rules.</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {rules.map((r, i) => (
              <li
                key={`${r.pattern}-${i}`}
                className="flex items-center gap-2 rounded-md bg-muted/50 py-1 pr-1 pl-2 font-mono text-xs"
              >
                <span className="min-w-0 flex-1 truncate" title={`${r.pattern} → ${r.replacement}`}>
                  {r.pattern} → {r.replacement}
                </span>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label={`Remove rule ${r.pattern}`}
                  disabled={saving}
                  onClick={() => void save(rules.filter((_, j) => j !== i))}
                >
                  <Trash2 />
                </Button>
              </li>
            ))}
          </ul>
        )}
        <form
          className="mt-auto grid gap-2"
          onSubmit={async (e) => {
            e.preventDefault();
            if (await save([...rules, { pattern, replacement }])) {
              setPattern("");
              setReplacement("");
            }
          }}
        >
          <Label htmlFor={`${id}-pattern`} className="text-xs">
            Pattern (regex)
          </Label>
          <Input
            id={`${id}-pattern`}
            placeholder="^/api/v\d+"
            value={pattern}
            onChange={(e) => setPattern(e.target.value)}
            className="font-mono text-xs"
            aria-invalid={error ? true : undefined}
            aria-describedby={error ? `${id}-error` : undefined}
          />
          <Label htmlFor={`${id}-replacement`} className="text-xs">
            Replacement
          </Label>
          <Input
            id={`${id}-replacement`}
            placeholder="/api/v:ver"
            value={replacement}
            onChange={(e) => setReplacement(e.target.value)}
            className="font-mono text-xs"
          />
          {error && (
            <p id={`${id}-error`} role="alert" className="text-xs text-status-critical">
              {error}
            </p>
          )}
          <Button
            type="submit"
            size="sm"
            variant="secondary"
            disabled={!pattern || !replacement || saving}
          >
            <Plus data-icon="inline-start" />
            Add rule
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}

function RouteButton({
  route,
  selected,
  onSelect,
}: {
  route: string;
  selected: boolean;
  onSelect: (route: string) => void;
}) {
  return (
      <span className="flex min-w-0 items-center gap-2">
        <button
          type="button"
          onClick={() => onSelect(route)}
          aria-pressed={selected}
          title={`Show latency for ${route}`}
          className={cn(
            "block max-w-full truncate rounded-sm px-1 text-left font-mono hover:underline",
            selected && "font-semibold text-primary",
          )}
        >
          {route}
        </button>
        <Link
          to={`/traces?operation=${encodeURIComponent(route)}`}
          className="shrink-0 font-sans text-[11px] text-primary underline-offset-2 hover:underline"
          aria-label={`Traces for ${route}`}
        >
          Traces
        </Link>
      </span>
  );
}

function EndpointTable({
  caption,
  rows,
  metric,
  selected,
  onSelect,
}: {
  caption: string;
  rows: EndpointRow[];
  metric: "p95" | "error";
  selected: string | undefined;
  onSelect: (route: string) => void;
}) {
  if (rows.length === 0) {
    return (
      <p className="py-4 text-sm text-muted-foreground">
        {metric === "error" ? "No failing endpoints." : "No requests yet."}
      </p>
    );
  }
  return (
    <table className="w-full table-fixed text-left text-xs">
      <caption className="sr-only">{caption}</caption>
      <thead className="text-muted-foreground">
        <tr className="border-b border-border">
          <th scope="col" className="pb-2 font-medium">Route</th>
          <th scope="col" className="w-24 pb-2 text-right font-medium">
            {metric === "error" ? "Error rate" : "p95"}
          </th>
          <th scope="col" className="w-24 pb-2 text-right font-medium">Throughput</th>
        </tr>
      </thead>
      <tbody className="font-mono">
        {rows.map((r) => (
          <tr key={r.route} className="border-b border-border/60 last:border-0">
            <td className="py-2 pr-2">
              <RouteButton route={r.route} selected={r.route === selected} onSelect={onSelect} />
            </td>
            <td className="py-2 text-right">
              {metric === "error" ? formatErr(r.errorRate) : formatMs(r.p95Ms)}
            </td>
            <td className="py-2 text-right">{formatRate(r.throughput)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function ApmEndpointsPage() {
  const { servers, selectedServerId, statusById } = useServersContext();
  const { isLive } = useTimeRange();
  const connection = selectedServerId ? statusById.get(selectedServerId)?.status : undefined;
  const connected = connection === "connected" || connection === "degraded";
  const {
    endpoints,
    heatmap,
    slowest,
    mostFailing,
    latencySeries,
    routeRules,
    saveRouteRules,
    timingNotice,
    loading,
  } = useApmEndpoints(selectedServerId);
  const [chosenRoute, setChosenRoute] = useRememberedState(`apm:${selectedServerId ?? "none"}:route`, "");

  if (!selectedServerId) {
    return (
      <EmptyState
        title="No server selected"
        description="Select a server in the sidebar and connect to see per-endpoint latency, throughput and errors."
      />
    );
  }

  if (isLive && !connected) {
    return (
      <EmptyState
        title="Server not connected"
        description="Connect the selected server from the Overview page to stream endpoint metrics."
      />
    );
  }

  const routes = endpoints.map((e) => e.route);
  const activeRoute =
    chosenRoute && routes.includes(chosenRoute) ? chosenRoute : routes[0];
  const activeSeries = latencySeries.find((s) => s.route === activeRoute);

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title="API endpoints"
        description={`${servers.find((item) => item.id === selectedServerId)?.name ?? "This server"}. Latency, throughput, and errors per route.`}
      />
      {!isLive && (
        <Notice tone="info">
          Endpoint metrics are computed from the live log stream. Switch the time
          range to Live to see them.
        </Notice>
      )}
      {connection === "degraded" && (
        <Notice tone="warning">Connection is degraded; updates may be delayed.</Notice>
      )}
      {timingNotice && (
        <Notice
          tone="warning"
          title={timingNotice.message}
          action={<HowToFix text="nginx timing log" />}
        >
          <details className="mt-2">
            <summary className="cursor-pointer text-xs">Show what this will run</summary>
            <pre className="mt-2 overflow-x-auto rounded-md bg-background/60 p-2 font-mono text-xs">
              {timingNotice.suggestedLine}
            </pre>
          </details>
        </Notice>
      )}

      <div className="grid gap-6 lg:grid-cols-3">
        {loading ? (
          <Skeleton className="h-[328px] w-full rounded-xl lg:col-span-2" />
        ) : (
          <LatencyBandChart
            title="Latency percentiles"
            points={activeSeries?.points ?? []}
            height={248}
            className="lg:col-span-2"
            action={
              <Select
                value={activeRoute ?? ""}
                onValueChange={(v) => setChosenRoute(v as string)}
                disabled={routes.length === 0}
              >
                <SelectTrigger
                  className="w-64 max-w-[50%] font-mono text-xs"
                  aria-label="Endpoint shown in latency chart"
                >
                  <SelectValue placeholder="No endpoints yet" />
                </SelectTrigger>
                <SelectContent>
                  {routes.map((r) => (
                    <SelectItem key={r} value={r} className="min-h-8 font-mono text-xs">
                      {r}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            }
          />
        )}
        <RouteRulesCard rules={routeRules} onSave={saveRouteRules} />
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Latency heatmap (p95 per minute)</CardTitle>
        </CardHeader>
        <CardContent>
          {loading ? (
            <Skeleton className="h-32 w-full" />
          ) : (
            <LatencyHeatmap cells={heatmap} routes={routes} />
          )}
        </CardContent>
      </Card>

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Slowest endpoints (p95)</CardTitle>
          </CardHeader>
          <CardContent>
            {loading ? (
              <Skeleton className="h-40 w-full" />
            ) : (
              <EndpointTable
                caption="Slowest endpoints by p95 latency"
                rows={slowest}
                metric="p95"
                selected={activeRoute}
                onSelect={setChosenRoute}
              />
            )}
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Most failing (5xx)</CardTitle>
          </CardHeader>
          <CardContent>
            {loading ? (
              <Skeleton className="h-40 w-full" />
            ) : (
              <EndpointTable
                caption="Endpoints with the highest 5xx error rate"
                rows={mostFailing}
                metric="error"
                selected={activeRoute}
                onSelect={setChosenRoute}
              />
            )}
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">All endpoints</CardTitle>
        </CardHeader>
        <CardContent>
          {loading ? (
            <Skeleton className="h-40 w-full" />
          ) : endpoints.length === 0 ? (
            <p className="py-4 text-sm text-muted-foreground">
              No requests parsed yet. Endpoints appear as lines arrive from an nginx
              or Apache access log the SSH user can read.
              <HowToFix text="nginx access log not readable" />
            </p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full min-w-[880px] table-fixed text-left text-xs">
                <caption className="sr-only">All endpoints with latency and status breakdown</caption>
                <thead className="text-muted-foreground">
                  <tr className="border-b border-border">
                    <th scope="col" className="pb-2 font-medium">Route</th>
                    <th scope="col" className="w-24 pb-2 text-right font-medium">Throughput</th>
                    <th scope="col" className="w-20 pb-2 text-right font-medium">avg</th>
                    <th scope="col" className="w-20 pb-2 text-right font-medium">p50</th>
                    <th scope="col" className="w-20 pb-2 text-right font-medium">p95</th>
                    <th scope="col" className="w-20 pb-2 text-right font-medium">p99</th>
                    <th scope="col" className="w-20 pb-2 text-right font-medium">Errors</th>
                    <th scope="col" className="w-56 pb-2 pl-4 font-medium">Status codes</th>
                  </tr>
                </thead>
                <tbody className="font-mono">
                  {endpoints.map((ep) => (
                    <tr key={ep.route} className="border-b border-border/60 align-top last:border-0">
                      <td className="py-2 pr-2">
                        <RouteButton
                          route={ep.route}
                          selected={ep.route === activeRoute}
                          onSelect={setChosenRoute}
                        />
                      </td>
                      <td className="py-2 text-right">{formatRate(ep.throughput)}</td>
                      <td className="py-2 text-right">{formatMs(ep.avgMs)}</td>
                      <td className="py-2 text-right">{formatMs(ep.p50Ms)}</td>
                      <td className="py-2 text-right">{formatMs(ep.p95Ms)}</td>
                      <td className="py-2 text-right">{formatMs(ep.p99Ms)}</td>
                      <td
                        className={cn(
                          "py-2 text-right",
                          ep.errorRate > 0.05 && "font-semibold text-status-critical",
                        )}
                      >
                        {formatErr(ep.errorRate)}
                      </td>
                      <td className="py-2 pl-4">
                        <StatusStackBar breakdown={familyBreakdown(ep.statusBreakdown)} compact />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>

    </div>
  );
}
