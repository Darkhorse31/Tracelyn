import { Fragment, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { toast } from "sonner";
import { Sparkline } from "@/components/charts/sparkline";
import { Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Masked } from "@/contexts/privacy-context";
import { listErrorIssues, setErrorIssueState } from "@/lib/api";
import type { ErrorIssue } from "@/lib/insight-types";

function when(ms: number) {
  if (!ms) return "—";
  return new Date(ms).toLocaleString();
}

export function ErrorIssues({
  serverId,
  appId,
  traceId,
}: {
  serverId: string;
  appId?: string;
  traceId?: string;
}) {
  const [rows, setRows] = useState<ErrorIssue[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [stateFilter, setStateFilter] = useState("open");
  const [openId, setOpenId] = useState<string | null>(null);

  const load = () => {
    void listErrorIssues(serverId, appId)
      .then((next) => {
        setRows(next);
        setError(null);
      })
      .catch((err) => setError(String(err)));
  };

  useEffect(() => {
    load();
    const timer = setInterval(load, 5000);
    return () => clearInterval(timer);
  }, [serverId, appId]);

  const setState = async (fingerprint: string, issueState: string) => {
    try {
      await setErrorIssueState(serverId, fingerprint, issueState);
      load();
    } catch (err) {
      toast.error(String(err));
    }
  };

  if (error) {
    return (
      <Notice tone="critical" title="Could not load errors">
        {error}
      </Notice>
    );
  }
  if (!rows) return <Skeleton className="h-40 w-full" />;

  const visible = rows.filter((row) => {
    if (stateFilter !== "all" && row.state !== stateFilter) return false;
    if (traceId && !row.traceIds.includes(traceId)) return false;
    return true;
  });

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2" role="group" aria-label="Error state">
        {["open", "resolved", "ignored", "all"].map((name) => (
          <Button
            key={name}
            type="button"
            size="sm"
            variant={stateFilter === name ? "secondary" : "outline"}
            aria-pressed={stateFilter === name}
            onClick={() => setStateFilter(name)}
          >
            {name}
          </Button>
        ))}
      </div>
      {visible.length === 0 ? (
        <Notice title="No grouped errors">
          {stateFilter === "open"
            ? "Error lines from application logs and OpenTelemetry exception events show up here after they are read. Stack traces are grouped with line numbers and ids removed."
            : `Nothing is marked ${stateFilter}.`}
        </Notice>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full min-w-[760px] text-left text-sm">
            <caption className="sr-only">Grouped errors</caption>
            <thead className="text-xs text-muted-foreground">
              <tr className="border-b border-border">
                <th scope="col" className="pb-2 font-medium">Issue</th>
                <th scope="col" className="pb-2 font-medium">Language</th>
                <th scope="col" className="pb-2 text-right font-medium">Count</th>
                <th scope="col" className="pb-2 font-medium">Last day</th>
                <th scope="col" className="pb-2 font-medium">Last seen</th>
                <th scope="col" className="pb-2 font-medium">State</th>
              </tr>
            </thead>
            <tbody>
              {visible.map((row) => (
                <Fragment key={row.fingerprint}>
                  <tr className="border-b border-border/60 align-top">
                    <th scope="row" className="py-2 pr-3 font-normal">
                      <button
                        type="button"
                        className="text-left hover:underline"
                        aria-expanded={openId === row.fingerprint}
                        onClick={() => setOpenId(openId === row.fingerprint ? null : row.fingerprint)}
                      >
                        <Masked text={row.title || row.sampleMessage} />
                      </button>
                      <span className="mt-1 block text-xs text-muted-foreground">
                        First seen {when(row.firstSeenMs)}
                        {row.appId ? ` · app ${row.appId}` : ""}
                      </span>
                    </th>
                    <td className="py-2 pr-3">{row.language}</td>
                    <td className="py-2 pr-3 text-right tabular-nums">{row.count}</td>
                    <td className="py-2 pr-3">
                      <Sparkline values={row.sparkline} max={Math.max(1, ...row.sparkline)} color={row.state === "open" ? "vermillion" : "blue"} />
                    </td>
                    <td className="py-2 pr-3 whitespace-nowrap">{when(row.lastSeenMs)}</td>
                    <td className="py-2">
                      <span className="mr-2 text-xs uppercase tracking-wide">{row.state}</span>
                      {row.state !== "resolved" && (
                        <Button type="button" size="sm" variant="outline" onClick={() => void setState(row.fingerprint, "resolved")}>
                          Resolve
                        </Button>
                      )}
                      {row.state !== "ignored" && (
                        <Button type="button" size="sm" variant="ghost" onClick={() => void setState(row.fingerprint, "ignored")}>
                          Ignore
                        </Button>
                      )}
                      {row.state !== "open" && (
                        <Button type="button" size="sm" variant="ghost" onClick={() => void setState(row.fingerprint, "open")}>
                          Reopen
                        </Button>
                      )}
                    </td>
                  </tr>
                  {openId === row.fingerprint && (
                    <tr className="border-b border-border/60">
                      <td colSpan={6} className="bg-muted/30 px-3 py-3">
                        <p className="text-sm"><Masked text={row.sampleMessage} /></p>
                        <pre className="mt-2 max-h-48 overflow-auto font-mono text-xs"><Masked text={row.sampleStack} /></pre>
                        <div className="mt-2 flex flex-wrap gap-3 text-sm">
                          {row.traceIds.map((id) => (
                            <Link key={id} className="text-primary underline-offset-2 hover:underline" to={`/traces/${encodeURIComponent(id)}`}>
                              Trace {id.slice(0, 8)}
                            </Link>
                          ))}
                          {row.traceIds[0] && (
                            <Link className="text-primary underline-offset-2 hover:underline" to={`/logs?q=${encodeURIComponent(row.traceIds[0])}`}>
                              Related logs
                            </Link>
                          )}
                        </div>
                      </td>
                    </tr>
                  )}
                </Fragment>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
