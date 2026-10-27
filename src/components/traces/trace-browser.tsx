import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useRememberedState } from "@/hooks/use-remembered-state";
import { Link, useSearchParams } from "react-router-dom";
import { EmptyState, Notice } from "@/components/status/notice";
import { Button, buttonVariants } from "@/components/ui/button";
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
  const scope = `traces:${serverId}:${appId ?? "all"}`;
  const [service, setService] = useRememberedState(
    `${scope}:service`,
    params.get("service") ?? "",
  );
  const [operation, setOperation] = useRememberedState(
    `${scope}:operation`,
    params.get("operation") ?? "",
  );
  const [status, setStatus] = useRememberedState(
    `${scope}:status`,
    params.get("status") ?? "",
  );
  const [minDuration, setMinDuration] = useRememberedState(
    `${scope}:min`,
    params.get("min") ?? "",
  );
  const [traceId, setTraceId] = useRememberedState(
    `${scope}:trace`,
    params.get("trace") ?? "",
  );
  const [rows, setRows] = useState<TraceSummary[] | null>(null);
  const [setup, setSetup] = useState<InsightSetup | null>(null);
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [showAll, setShowAll] = useRememberedState("traces:show-all", false);
  const virtualizer = useVirtualizer({
    count: rows?.length ?? 0,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 44,
    getItemKey: (index) => rows?.[index]?.traceId ?? index,
    overscan: 10,
    enabled: !showAll,
  });
  const virtualRows = virtualizer.getVirtualItems();
  const shownRows = showAll
    ? (rows ?? []).map((row, index) => ({ row, index }))
    : virtualRows.map((item) => ({
        row: rows![item.index],
        index: item.index,
      }));

  useEffect(() => {
    if (
      !["service", "operation", "status", "min", "trace"].some((key) =>
        params.has(key),
      )
    )
      return;
    setService(params.get("service") ?? "");
    setOperation(params.get("operation") ?? "");
    setStatus(params.get("status") ?? "");
    setMinDuration(params.get("min") ?? "");
    setTraceId(params.get("trace") ?? "");
  }, [params, setService, setOperation, setStatus, setMinDuration, setTraceId]);

  useEffect(() => {
    let cancelled = false;
    setRows(null);
    setError(null);
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
    const timer = isLive
      ? setInterval(() => {
          load();
          loadSetup();
        }, 5000)
      : undefined;
    return () => {
      cancelled = true;
      if (timer) clearInterval(timer);
    };
  }, [
    serverId,
    appId,
    service,
    operation,
    status,
    minDuration,
    traceId,
    isLive,
    range.startMs,
    range.endMs,
  ]);

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
          <Input
            id="trace-service"
            value={service}
            onChange={(event) => setService(event.target.value)}
            className="mt-1"
          />
        </div>
        <div>
          <Label htmlFor="trace-operation">Operation</Label>
          <Input
            id="trace-operation"
            value={operation}
            onChange={(event) => setOperation(event.target.value)}
            className="mt-1"
          />
        </div>
        <div>
          <Label htmlFor="trace-status">Status</Label>
          <Input
            id="trace-status"
            value={status}
            onChange={(event) => setStatus(event.target.value)}
            placeholder="error"
            className="mt-1"
          />
        </div>
        <div>
          <Label htmlFor="trace-min">Min duration (ms)</Label>
          <Input
            id="trace-min"
            value={minDuration}
            onChange={(event) => setMinDuration(event.target.value)}
            inputMode="decimal"
            className="mt-1"
          />
        </div>
        <div>
          <Label htmlFor="trace-id">Trace id</Label>
          <Input
            id="trace-id"
            value={traceId}
            onChange={(event) => setTraceId(event.target.value)}
            className="mt-1 font-mono"
          />
        </div>
        <div className="flex flex-wrap items-center gap-3 md:col-span-5">
          <Button type="submit">Apply filters</Button>
          <Button
            type="button"
            variant="ghost"
            onClick={() => {
              setService("");
              setOperation("");
              setStatus("");
              setMinDuration("");
              setTraceId("");
              const next = new URLSearchParams(params);
              for (const key of [
                "service",
                "operation",
                "status",
                "min",
                "trace",
              ])
                next.delete(key);
              setParams(next);
            }}
          >
            Clear filters
          </Button>
          <span className="text-xs text-muted-foreground">
            Filters are remembered for this server
          </span>
        </div>
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
            <Link className={buttonVariants({ variant: "outline" })} to="/setup?step=otel">
              Open the Setup Guide
            </Link>
          }
        />
      )}
      {rows && rows.length === 0 && setup?.stats.sample && (
        <pre
          tabIndex={0}
          className="overflow-auto rounded-md border border-border bg-muted/40 p-3 font-mono text-xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <Masked text={setup.stats.sample} />
        </pre>
      )}
      {rows && rows.length > 0 && (
        <>
          <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
            <p role="status">{rows.length} traces</p>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                className="size-4 accent-primary"
                checked={showAll}
                onChange={(event) => setShowAll(event.target.checked)}
              />
              Show all results
            </label>
          </div>
          <div
            ref={scrollRef}
            role="region"
            aria-label="Trace results"
            tabIndex={0}
            className="max-h-[560px] overflow-auto rounded-lg border border-border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <table
              className="w-full min-w-[720px] text-left text-sm"
              aria-rowcount={rows.length + 1}
            >
              <caption className="sr-only">Traces</caption>
              <thead className="sticky top-0 z-10 bg-card text-xs text-muted-foreground">
                <tr className="border-b border-border">
                  <th scope="col" className="pb-2 font-medium">
                    When
                  </th>
                  <th scope="col" className="pb-2 font-medium">
                    Service
                  </th>
                  <th scope="col" className="pb-2 font-medium">
                    Operation
                  </th>
                  <th scope="col" className="pb-2 font-medium">
                    Status
                  </th>
                  <th scope="col" className="pb-2 text-right font-medium">
                    Duration
                  </th>
                  <th scope="col" className="pb-2 text-right font-medium">
                    Spans
                  </th>
                </tr>
              </thead>
              <tbody>
                {!showAll && virtualRows.length > 0 && (
                  <tr aria-hidden>
                    <td
                      colSpan={6}
                      style={{ height: virtualRows[0].start, padding: 0 }}
                    />
                  </tr>
                )}
                {shownRows.map(({ row, index }) => (
                  <tr
                    key={row.traceId}
                    aria-rowindex={index + 2}
                    data-index={index}
                    ref={showAll ? undefined : virtualizer.measureElement}
                    className="border-b border-border/60"
                  >
                    <td className="py-2 pr-3 whitespace-nowrap">
                      {new Date(row.startMs).toLocaleString()}
                    </td>
                    <td className="py-2 pr-3">
                      <Masked text={row.service} />
                    </td>
                    <td className="py-2 pr-3">
                      <Link
                        className="font-mono text-primary underline-offset-2 hover:underline"
                        to={`/traces/${encodeURIComponent(row.traceId)}`}
                      >
                        <Masked text={row.operation} />
                      </Link>
                    </td>
                    <td className="py-2 pr-3">
                      <span
                        className={
                          row.status === "error"
                            ? "font-medium text-status-critical"
                            : ""
                        }
                      >
                        {row.status}
                      </span>
                    </td>
                    <td className="py-2 pr-3 text-right tabular-nums">
                      {formatDur(row.durationUs)}
                    </td>
                    <td className="py-2 text-right tabular-nums">
                      {row.spanCount}
                    </td>
                  </tr>
                ))}
                {!showAll && virtualRows.length > 0 && (
                  <tr aria-hidden>
                    <td
                      colSpan={6}
                      style={{
                        height: Math.max(
                          0,
                          virtualizer.getTotalSize() -
                            virtualRows[virtualRows.length - 1].end,
                        ),
                        padding: 0,
                      }}
                    />
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  );
}
