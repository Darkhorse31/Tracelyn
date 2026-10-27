import { useEffect, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { EmptyState, Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { Masked } from "@/contexts/privacy-context";
import { useTimeRange } from "@/contexts/time-range-context";
import { getInsightSetup, listTraces } from "@/lib/api";
import type { InsightSetup, TraceSummary } from "@/lib/insight-types";
import { formatDur } from "@/components/traces/waterfall";

function windowFor(isLive: boolean, startMs: number, endMs: number) {
  if (!isLive) return { startMs, endMs };
  const end = Date.now();
  return { startMs: end - 15 * 60 * 1000, endMs: end };
}

export function TraceBrowser({
  serverId,
  appId,
}: {
  serverId: string;
  appId?: string;
}) {
  const { range, isLive } = useTimeRange();
  const [params, setParams] = useSearchParams();
  const [service, setService] = useState(params.get("service") ?? "");
  const [operation, setOperation] = useState(params.get("operation") ?? "");
  const [status, setStatus] = useState(params.get("status") ?? "");
  const [minDuration, setMinDuration] = useState(params.get("min") ?? "");
  const [traceId, setTraceId] = useState(params.get("trace") ?? "");
  const [rows, setRows] = useState<TraceSummary[] | null>(null);
  const [setup, setSetup] = useState<InsightSetup | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setService(params.get("service") ?? "");
    setOperation(params.get("operation") ?? "");
    setStatus(params.get("status") ?? "");
    setMinDuration(params.get("min") ?? "");
    setTraceId(params.get("trace") ?? "");
  }, [params]);

  useEffect(() => {
    let cancelled = false;
    const load = () => {
      const span = windowFor(isLive, range.startMs, range.endMs);
      const min = Number(minDuration);
      void listTraces({
        serverId,
        startMs: span.startMs,
        endMs: span.endMs,
        service: service || undefined,
        operation: operation || undefined,
        status: status || undefined,
        minDurationMs: Number.isFinite(min) && min > 0 ? min : undefined,
        traceId: traceId || undefined,
        appId,
      })
        .then((next) => {
          if (!cancelled) {
            setRows(next);
            setError(null);
          }
        })
        .catch((err) => {
          if (!cancelled) setError(String(err));
        });
    };
    const loadSetup = () => {
      void getInsightSetup(serverId)
        .then((next) => {
          if (!cancelled) setSetup(next);
        })
        .catch(() => {
          if (!cancelled) setSetup(null);
        });
    };
    load();
    loadSetup();
    const timer = isLive ? setInterval(() => {
      load();
      loadSetup();
    }, 5000) : undefined;
    return () => {
      cancelled = true;
      if (timer) clearInterval(timer);
    };
  }, [serverId, appId, service, operation, status, minDuration, traceId, isLive, range.startMs, range.endMs]);

  return (
    <div className="flex flex-col gap-4">
      <form
        className="grid gap-3 md:grid-cols-5"
        onSubmit={(event) => {
          event.preventDefault();
          const next = new URLSearchParams();
          if (service) next.set("service", service);
          if (operation) next.set("operation", operation);
          if (status) next.set("status", status);
          if (minDuration) next.set("min", minDuration);
          if (traceId) next.set("trace", traceId);
          setParams(next);
        }}
      >
        <div>
          <Label htmlFor="trace-service">Service</Label>
          <Input id="trace-service" value={service} onChange={(event) => setService(event.target.value)} className="mt-1" />
        </div>
        <div>
          <Label htmlFor="trace-operation">Operation</Label>
          <Input id="trace-operation" value={operation} onChange={(event) => setOperation(event.target.value)} className="mt-1" />
        </div>
        <div>
          <Label htmlFor="trace-status">Status</Label>
          <Input id="trace-status" value={status} onChange={(event) => setStatus(event.target.value)} placeholder="error" className="mt-1" />
        </div>
        <div>
          <Label htmlFor="trace-min">Min duration (ms)</Label>
          <Input id="trace-min" value={minDuration} onChange={(event) => setMinDuration(event.target.value)} inputMode="decimal" className="mt-1" />
        </div>
        <div>
          <Label htmlFor="trace-id">Trace id</Label>
          <Input id="trace-id" value={traceId} onChange={(event) => setTraceId(event.target.value)} className="mt-1 font-mono" />
        </div>
        <Button type="submit" className="md:col-span-5 w-fit">
          Apply filters
        </Button>
      </form>
      {error && (
        <Notice tone="critical" title="Could not load traces">
          {error}
        </Notice>
      )}
      {!rows && !error && <Skeleton className="h-40 w-full" />}
      {rows && rows.length === 0 && (
        <EmptyState
          title="No traces in this range"
          description={
            setup?.stats.detail ||
            "No collector file has been read yet. Tracelyn tails the OpenTelemetry file exporter over SSH."
          }
          action={
            <Button variant="outline" render={<Link to="/setup?step=otel" />}>
              Open the Setup Guide
            </Button>
          }
        />
      )}
      {rows && rows.length === 0 && setup?.stats.sample && (
        <pre className="overflow-auto rounded-md border border-border bg-muted/40 p-3 font-mono text-xs">
          <Masked text={setup.stats.sample} />
        </pre>
      )}
      {rows && rows.length > 0 && (
        <div className="overflow-x-auto">
          <table className="w-full min-w-[720px] text-left text-sm">
            <caption className="sr-only">Traces</caption>
            <thead className="text-xs text-muted-foreground">
              <tr className="border-b border-border">
                <th scope="col" className="pb-2 font-medium">When</th>
                <th scope="col" className="pb-2 font-medium">Service</th>
                <th scope="col" className="pb-2 font-medium">Operation</th>
                <th scope="col" className="pb-2 font-medium">Status</th>
                <th scope="col" className="pb-2 text-right font-medium">Duration</th>
                <th scope="col" className="pb-2 text-right font-medium">Spans</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={row.traceId} className="border-b border-border/60">
                  <td className="py-2 pr-3 whitespace-nowrap">{new Date(row.startMs).toLocaleString()}</td>
                  <td className="py-2 pr-3">
                    <Masked text={row.service} />
                  </td>
                  <td className="py-2 pr-3">
                    <Link className="font-mono text-primary underline-offset-2 hover:underline" to={`/traces/${encodeURIComponent(row.traceId)}`}>
                      <Masked text={row.operation} />
                    </Link>
                  </td>
                  <td className="py-2 pr-3">
                    <span className={row.status === "error" ? "font-medium text-status-critical" : ""}>
                      {row.status}
                    </span>
                  </td>
                  <td className="py-2 pr-3 text-right tabular-nums">{formatDur(row.durationUs)}</td>
                  <td className="py-2 text-right tabular-nums">{row.spanCount}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
