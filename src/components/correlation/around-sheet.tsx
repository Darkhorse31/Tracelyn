import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ChevronRight, Copy } from "lucide-react";
import { toast } from "sonner";
import { getLogSnapshot, listTraces, metricsAround } from "@/lib/api";
import type { TraceSummary } from "@/lib/insight-types";
import type { LogLine } from "@/lib/log-types";
import { firstLine, groupBody, groupLogLines, groupText, textLineCount, type LogGroup } from "@/lib/log-groups";
import { Masked, usePrivacy } from "@/contexts/privacy-context";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { cn } from "cn";

const WINDOW_MS = 2 * 60 * 1000;

type MetricSnap = { cpu: number | null; memory: number | null; load: number | null };

function formatMetric(value: number | null, digits: number, suffix = ""): string {
  if (value == null) return "n/a";
  return `${value.toFixed(digits)}${suffix}`;
}

function levelVariant(level: string): "destructive" | "outline" | "secondary" {
  if (level === "error" || level === "fatal") return "destructive";
  if (level === "warn" || level === "warning") return "outline";
  return "secondary";
}

function CopyButton({ text, label }: { text: string; label: string }) {
  const { mask } = usePrivacy();
  return (
    <Button
      type="button"
      size="icon"
      variant="ghost"
      className="size-8 shrink-0"
      aria-label={label}
      onClick={() =>
        void navigator.clipboard.writeText(mask(text)).then(
          () => toast.success("Copied"),
          () => toast.error("Could not copy"),
        )
      }
    >
      <Copy />
    </Button>
  );
}

function EntryBody({
  id,
  text,
  expanded,
  onToggle,
}: {
  id: string;
  text: string;
  expanded: boolean;
  onToggle: () => void;
}) {
  const count = textLineCount(text);
  const open = expanded && count > 1;
  return (
    <div className="min-w-0 flex-1">
      <p className="font-mono text-xs break-all whitespace-pre-wrap">
        <Masked text={open ? text : firstLine(text)} />
      </p>
      {count > 1 ? (
        <Button
          type="button"
          size="sm"
          variant="ghost"
          className="mt-2 h-8 px-2"
          aria-expanded={open}
          aria-controls={id}
          onClick={onToggle}
        >
          <ChevronRight aria-hidden className={cn("size-4 transition-transform", open && "rotate-90")} />
          {open ? "Collapse" : `Expand ${count - 1} more`}
        </Button>
      ) : null}
    </div>
  );
}

function LogEntry({ group, expanded, onToggle }: { group: LogGroup; expanded: boolean; onToggle: () => void }) {
  const text = groupBody(group);
  return (
    <li className="rounded-lg border border-border p-4">
      <div className="flex items-start gap-2">
        <time className="shrink-0 pt-0.5 text-xs text-muted-foreground tabular-nums" dateTime={new Date(group.tsMs).toISOString()}>
          {new Date(group.tsMs).toLocaleTimeString()}
        </time>
        <Badge variant={levelVariant(group.level)} className="uppercase">
          {group.level}
        </Badge>
        <EntryBody id={`around-log-${group.id}`} text={text} expanded={expanded} onToggle={onToggle} />
        <CopyButton text={text} label="Copy log entry" />
      </div>
    </li>
  );
}

function ErrorEntry({ text }: { text: string }) {
  const [open, setOpen] = useState<Record<number, boolean>>({});
  const groups = groupText(text);
  return (
    <section className="rounded-xl bg-card p-4 text-card-foreground ring-1 ring-foreground/10">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">Error</h3>
        <Badge variant="secondary">{groups.length}</Badge>
      </div>
      <ul className="mt-4 flex flex-col gap-2">
        {groups.map((rows, index) => {
          const body = rows.join("\n");
          return (
            <li key={index} className="flex items-start gap-2 rounded-lg border border-border p-4">
              <Badge variant="destructive" className="uppercase">
                Error
              </Badge>
              <EntryBody
                id={`around-error-${index}`}
                text={body}
                expanded={Boolean(open[index])}
                onToggle={() => setOpen((prev) => ({ ...prev, [index]: !prev[index] }))}
              />
              <CopyButton text={body} label="Copy error" />
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function MetricChips({ metrics }: { metrics: MetricSnap }) {
  const chips = [
    ["CPU", formatMetric(metrics.cpu, 1, "%")],
    ["Memory", formatMetric(metrics.memory, 1, "%")],
    ["Load", formatMetric(metrics.load, 2)],
  ] as const;
  return (
    <section className="rounded-xl bg-card p-4 text-card-foreground ring-1 ring-foreground/10">
      <h3 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">Metrics</h3>
      <div className="mt-4 flex flex-wrap gap-2">
        {chips.map(([label, value]) => (
          <div key={label} className="flex h-8 items-center gap-2 rounded-lg border border-border bg-muted/40 px-4">
            <span className="text-xs text-muted-foreground">{label}</span>
            <span className="font-mono text-sm font-medium">{value}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

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
  const [metrics, setMetrics] = useState<MetricSnap | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [openIds, setOpenIds] = useState<Record<string, boolean>>({});

  useEffect(() => {
    if (timestampMs == null) return;
    let cancelled = false;
    const start = timestampMs - WINDOW_MS;
    const end = timestampMs + WINDOW_MS;
    setError(null);
    setOpenIds({});
    void Promise.all([
      getLogSnapshot(serverId),
      listTraces({ serverId, startMs: start, endMs: end }),
      metricsAround(serverId, timestampMs, WINDOW_MS),
    ])
      .then(([snap, found, next]) => {
        if (cancelled) return;
        const lines = (snap?.lines ?? []).filter((line) => line.tsMs >= start && line.tsMs <= end).slice(-40);
        setLogs(lines);
        setTraces(found.slice(0, 40));
        setMetrics({
          cpu: next.cpu.points[next.cpu.points.length - 1]?.value ?? null,
          memory: next.memory.points[next.memory.points.length - 1]?.value ?? null,
          load: next.load.points[next.load.points.length - 1]?.value ?? null,
        });
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [serverId, timestampMs]);

  const groups = groupLogLines(logs);
  const when = timestampMs == null ? "" : new Date(timestampMs).toLocaleString();

  return (
    <Sheet open={timestampMs != null} onOpenChange={(open) => !open && onClose()}>
      <SheetContent className="w-full gap-0 overflow-hidden p-0 sm:max-w-md" side="right">
        <SheetHeader className="shrink-0 gap-2 border-b border-border px-6 py-6 pr-14">
          <SheetTitle>Around this time</SheetTitle>
          <SheetDescription>
            {when ? `${when}. Logs and traces from two minutes on either side.` : "Logs and traces from two minutes on either side."}
          </SheetDescription>
        </SheetHeader>
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto px-6 py-6">
          {error ? <ErrorEntry text={error} /> : null}
          {metrics ? <MetricChips metrics={metrics} /> : null}
          <section className="rounded-xl bg-card p-4 text-card-foreground ring-1 ring-foreground/10">
            <div className="flex items-center justify-between gap-2">
              <h3 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">Logs</h3>
              <Badge variant="secondary">{groups.length}</Badge>
            </div>
            {groups.length === 0 ? (
              <p className="mt-4 rounded-lg border border-dashed border-border px-4 py-6 text-sm text-muted-foreground">
                No log lines in this window.
              </p>
            ) : (
              <ul className="mt-4 flex flex-col gap-2">
                {groups.map((group) => (
                  <LogEntry
                    key={group.id}
                    group={group}
                    expanded={Boolean(openIds[group.id])}
                    onToggle={() => setOpenIds((prev) => ({ ...prev, [group.id]: !prev[group.id] }))}
                  />
                ))}
              </ul>
            )}
          </section>
          <section className="rounded-xl bg-card p-4 text-card-foreground ring-1 ring-foreground/10">
            <div className="flex items-center justify-between gap-2">
              <h3 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">Traces</h3>
              <Badge variant="secondary">{traces.length}</Badge>
            </div>
            {traces.length === 0 ? (
              <p className="mt-4 rounded-lg border border-dashed border-border px-4 py-6 text-sm text-muted-foreground">
                No traces in this window.
              </p>
            ) : (
              <ul className="mt-4 flex flex-col gap-2">
                {traces.map((trace) => (
                  <li key={trace.traceId} className="rounded-lg border border-border p-4">
                    <div className="flex flex-wrap items-center gap-2">
                      <time
                        className="text-xs text-muted-foreground tabular-nums"
                        dateTime={new Date(trace.startMs).toISOString()}
                      >
                        {new Date(trace.startMs).toLocaleTimeString()}
                      </time>
                      <Badge variant={levelVariant(trace.status)} className="uppercase">
                        {trace.status}
                      </Badge>
                      <Link
                        className="min-w-0 flex-1 font-mono text-xs text-primary underline-offset-2 hover:underline"
                        to={`/traces/${encodeURIComponent(trace.traceId)}`}
                      >
                        <Masked text={`${trace.service} ${trace.operation}`} />
                      </Link>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>
      </SheetContent>
    </Sheet>
  );
}
