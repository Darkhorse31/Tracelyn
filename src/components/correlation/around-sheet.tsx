import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getLogSnapshot, listTraces, metricsAround } from "@/lib/api";
import type { TraceSummary } from "@/lib/insight-types";
import type { LogLine } from "@/lib/log-types";
import { Masked } from "@/contexts/privacy-context";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";

const WINDOW_MS = 2 * 60 * 1000;

export function AroundSheet({
  serverId,
  timestampMs,
  onClose,
}: {
  serverId: string;
  timestampMs: number | null;
  onClose: () => void;
}) {
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [traces, setTraces] = useState<TraceSummary[]>([]);
  const [summary, setSummary] = useState<string>("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (timestampMs == null) return;
    let cancelled = false;
    const start = timestampMs - WINDOW_MS;
    const end = timestampMs + WINDOW_MS;
    setError(null);
    void Promise.all([
      getLogSnapshot(serverId),
      listTraces({ serverId, startMs: start, endMs: end }),
      metricsAround(serverId, timestampMs, WINDOW_MS),
    ])
      .then(([snap, found, metrics]) => {
        if (cancelled) return;
        const lines = (snap?.lines ?? []).filter((line) => line.tsMs >= start && line.tsMs <= end).slice(-40);
        setLogs(lines);
        setTraces(found.slice(0, 40));
        const cpu = metrics.cpu.points[metrics.cpu.points.length - 1]?.value;
        const mem = metrics.memory.points[metrics.memory.points.length - 1]?.value;
        const load = metrics.load.points[metrics.load.points.length - 1]?.value;
        setSummary(
          `CPU ${cpu == null ? "n/a" : cpu.toFixed(1) + "%"} · memory ${mem == null ? "n/a" : mem.toFixed(1) + "%"} · load ${load == null ? "n/a" : load.toFixed(2)}`,
        );
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [serverId, timestampMs]);

  return (
    <Sheet open={timestampMs != null} onOpenChange={(open) => !open && onClose()}>
      <SheetContent className="w-full sm:max-w-md" side="right">
        <SheetHeader>
          <SheetTitle>Around this time</SheetTitle>
          <SheetDescription>
            {timestampMs == null ? "" : new Date(timestampMs).toLocaleString()}
            {". Logs and traces from two minutes on either side."}
          </SheetDescription>
        </SheetHeader>
        {error && <p className="text-sm text-destructive">{error}</p>}
        {summary && <p className="font-mono text-xs text-foreground">{summary}</p>}
        <section className="min-h-0 flex-1 overflow-auto">
          <h3 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">Logs</h3>
          {logs.length === 0 ? (
            <p className="mt-2 text-sm text-muted-foreground">No log lines in this window.</p>
          ) : (
            <ul className="mt-2 flex flex-col gap-2">
              {logs.map((line) => (
                <li key={line.id} className="text-xs">
                  <span className="font-medium uppercase">{line.level}</span>{" "}
                  <Masked text={line.message} />
                </li>
              ))}
            </ul>
          )}
          <h3 className="mt-4 text-xs font-medium uppercase tracking-wide text-muted-foreground">Traces</h3>
          {traces.length === 0 ? (
            <p className="mt-2 text-sm text-muted-foreground">No traces in this window.</p>
          ) : (
            <ul className="mt-2 flex flex-col gap-2">
              {traces.map((trace) => (
                <li key={trace.traceId}>
                  <Link
                    className="text-sm text-primary underline-offset-2 hover:underline"
                    to={`/traces/${encodeURIComponent(trace.traceId)}`}
                  >
                    <Masked text={`${trace.service} ${trace.operation}`} />
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </section>
      </SheetContent>
    </Sheet>
  );
}
