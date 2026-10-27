import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { Waypoints } from "lucide-react";
import { Waterfall, formatDur } from "@/components/traces/waterfall";
import { EmptyState, Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Masked } from "@/contexts/privacy-context";
import { useServersContext } from "@/contexts/servers-context";
import { getTrace } from "@/lib/api";
import type { TraceDetail } from "@/lib/insight-types";

function textOf(value: unknown) {
  if (value == null) return "";
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

export function TraceDetailPage() {
  const { traceId = "" } = useParams();
  const { selectedServerId } = useServersContext();
  const [detail, setDetail] = useState<TraceDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [index, setIndex] = useState(0);

  useEffect(() => {
    if (!selectedServerId || !traceId) return;
    let cancelled = false;
    void getTrace(selectedServerId, decodeURIComponent(traceId))
      .then((next) => {
        if (!cancelled) {
          setDetail(next);
          setError(null);
          setIndex(0);
        }
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedServerId, traceId]);

  if (!selectedServerId) {
    return <EmptyState icon={Waypoints} title="No server selected" description="Select a server to open this trace." />;
  }
  if (error) {
    return (
      <Notice tone="critical" title="Could not load the trace">
        {error}
      </Notice>
    );
  }
  if (!detail) return <Skeleton className="h-80 w-full" />;
  if (detail.spans.length === 0) {
    return (
      <EmptyState
        icon={Waypoints}
        title="Trace not found"
        description="It may have aged out of retention. Traces follow the same raw history window as other metrics."
        action={<Button render={<Link to="/traces" />}>Back to traces</Button>}
      />
    );
  }

  const span = detail.spans[Math.min(index, detail.spans.length - 1)];
  const events = Array.isArray(span.events) ? span.events : [];

  return (
    <div className="flex flex-col gap-4">
      <div>
        <Button variant="ghost" size="sm" render={<Link to="/traces" />}>
          Traces
        </Button>
        <h2 className="mt-2 font-mono text-lg font-semibold tracking-tight">{detail.traceId}</h2>
        <p className="text-sm text-muted-foreground">
          {detail.spans.length} spans
          {detail.truncated ? " (showing the first 5,000)" : ""}. Arrow keys move through the waterfall.
        </p>
      </div>
      <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_22rem]">
        <Waterfall spans={detail.spans} selected={index} onSelect={setIndex} />
        <aside className="rounded-lg border border-border p-4" aria-label="Selected span">
          <h3 className="text-sm font-semibold">{span.operation}</h3>
          <dl className="mt-3 grid grid-cols-[7rem_1fr] gap-y-1 text-sm">
            <dt className="text-muted-foreground">Service</dt>
            <dd><Masked text={span.service} /></dd>
            <dt className="text-muted-foreground">Status</dt>
            <dd>{span.status}{span.critical ? " · critical path" : ""}</dd>
            <dt className="text-muted-foreground">Kind</dt>
            <dd>{span.kind}</dd>
            <dt className="text-muted-foreground">Duration</dt>
            <dd className="tabular-nums">{formatDur(span.durationUs)}</dd>
            {span.peer && (
              <>
                <dt className="text-muted-foreground">Peer</dt>
                <dd><Masked text={span.peer} /></dd>
              </>
            )}
          </dl>
          <div className="mt-3 flex flex-col gap-1 text-sm">
            <Link className="text-primary underline-offset-2 hover:underline" to={`/logs?q=${encodeURIComponent(detail.traceId)}`}>
              Logs with this trace id
            </Link>
            <Link className="text-primary underline-offset-2 hover:underline" to={`/errors?trace=${encodeURIComponent(detail.traceId)}`}>
              Errors in this trace
            </Link>
            {detail.errorFingerprints.map((id) => (
              <Link key={id} className="text-primary underline-offset-2 hover:underline" to={`/errors?trace=${encodeURIComponent(detail.traceId)}`}>
                Issue {id.slice(0, 8)}
              </Link>
            ))}
          </div>
          <h4 className="mt-4 text-xs font-medium uppercase tracking-wide text-muted-foreground">Attributes</h4>
          <dl className="mt-2 grid grid-cols-[8rem_1fr] gap-y-1 text-xs">
            {Object.entries(span.attributes ?? {}).length === 0 && <dd className="text-muted-foreground">None</dd>}
            {Object.entries(span.attributes ?? {}).map(([key, value]) => (
              <div key={key} className="contents">
                <dt className="truncate text-muted-foreground">{key}</dt>
                <dd className="truncate font-mono"><Masked text={textOf(value)} /></dd>
              </div>
            ))}
          </dl>
          <h4 className="mt-4 text-xs font-medium uppercase tracking-wide text-muted-foreground">Events</h4>
          {events.length === 0 ? (
            <p className="mt-2 text-xs text-muted-foreground">No events.</p>
          ) : (
            <ul className="mt-2 flex flex-col gap-2">
              {events.map((event, eventIndex) => {
                const stack = textOf(event.attributes?.["exception.stacktrace"]);
                return (
                <li key={`${event.name}-${eventIndex}`} className="text-xs">
                  <span className="font-medium">{event.name || "event"}</span>
                  {stack ? (
                    <pre className="mt-1 max-h-32 overflow-auto font-mono"><Masked text={stack} /></pre>
                  ) : null}
                </li>
                );
              })}
            </ul>
          )}
          {detail.logs.length > 0 && (
            <>
              <h4 className="mt-4 text-xs font-medium uppercase tracking-wide text-muted-foreground">Linked logs</h4>
              <ul className="mt-2 flex max-h-40 flex-col gap-1 overflow-auto">
                {detail.logs.map((log, logIndex) => (
                  <li key={`${log.tsMs}-${logIndex}`} className="text-xs">
                    <span className="font-medium">{log.severity}</span> <Masked text={log.body} />
                  </li>
                ))}
              </ul>
            </>
          )}
        </aside>
      </div>
    </div>
  );
}
