import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { EmptyState, Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { formatTs, SeverityBadge, statusLabel } from "@/components/intel/format";
import { useServersContext } from "@/contexts/servers-context";
import { listIncidents } from "@/pages/intel/api";
import type { Incident } from "@/pages/intel/types";

export function IncidentsPanel() {
  const { servers, selectedServerId } = useServersContext();
  const [scope, setScope] = useState<"selected" | "all">("selected");
  const [severity, setSeverity] = useState<"all" | "info" | "warning" | "critical">("all");
  const [status, setStatus] = useState<"all" | "open" | "ack" | "resolved">("all");
  const [rows, setRows] = useState<Incident[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const serverId = scope === "all" ? undefined : selectedServerId ?? undefined;

  useEffect(() => {
    let cancelled = false;
    setRows(null);
    listIncidents(serverId)
      .then((next) => {
        if (!cancelled) {
          setRows(next);
          setError(null);
        }
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [serverId]);

  const nameOf = (id: string) => servers.find((server) => server.id === id)?.name ?? id;
  const visible = (rows ?? []).filter((incident) => {
    if (severity !== "all" && incident.severity !== severity) return false;
    if (status !== "all" && incident.status !== status) return false;
    return true;
  });

  return (
    <Card>
      <CardHeader className="flex flex-row items-start justify-between gap-3">
        <div>
          <CardTitle className="text-base">Incidents</CardTitle>
          <CardDescription>
            Open alerts on the same server within 30 minutes share one incident. The timeline also
            includes PM2 restart jumps, anomalies, notes, and acknowledgements.
          </CardDescription>
        </div>
        <div className="flex flex-wrap justify-end gap-2" role="group" aria-label="Incident filters">
          <Button type="button" size="sm" variant={scope === "selected" ? "default" : "outline"} onClick={() => setScope("selected")} aria-pressed={scope === "selected"}>
            This server
          </Button>
          <Button type="button" size="sm" variant={scope === "all" ? "default" : "outline"} onClick={() => setScope("all")} aria-pressed={scope === "all"}>
            All servers
          </Button>
          {(["all", "info", "warning", "critical"] as const).map((value) => (
            <Button key={value} type="button" size="sm" variant={severity === value ? "default" : "outline"} onClick={() => setSeverity(value)} aria-pressed={severity === value}>
              {value === "all" ? "Any severity" : value.charAt(0).toUpperCase() + value.slice(1)}
            </Button>
          ))}
          {(["all", "open", "ack", "resolved"] as const).map((value) => (
            <Button key={`status-${value}`} type="button" size="sm" variant={status === value ? "default" : "outline"} onClick={() => setStatus(value)} aria-pressed={status === value}>
              {value === "all" ? "Any status" : statusLabel(value)}
            </Button>
          ))}
        </div>
      </CardHeader>
      <CardContent>
        {scope === "selected" && !selectedServerId && (
          <EmptyState title="No server selected" description="Select a server in the sidebar, or show incidents for all servers." />
        )}
        {error && (
          <Notice tone="critical" title="Could not load incidents">
            {error}
          </Notice>
        )}
        {(scope === "all" || selectedServerId) && rows === null && !error && <Skeleton className="h-24 w-full" />}
        {rows && rows.length === 0 && (
          <EmptyState title="No incidents" description="Rules that fire, and alerts from the Alerts page, are grouped here." />
        )}
        {rows && rows.length > 0 && visible.length === 0 && (
          <EmptyState title="No matching incidents" description="Change the severity or status filter to see more history." />
        )}
        {visible.length > 0 && (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <caption className="sr-only">Incidents</caption>
              <thead>
                <tr className="border-b border-border text-xs text-muted-foreground">
                  <th scope="col" className="py-2 pr-3 font-medium">Incident</th>
                  <th scope="col" className="py-2 pr-3 font-medium">Server</th>
                  <th scope="col" className="py-2 pr-3 font-medium">Severity</th>
                  <th scope="col" className="py-2 pr-3 font-medium">Status</th>
                  <th scope="col" className="py-2 font-medium">Updated</th>
                </tr>
              </thead>
              <tbody>
                {visible.map((incident) => (
                  <tr key={incident.id} className="border-b border-border/70">
                    <th scope="row" className="py-2 pr-3 font-medium">
                      <Link className="underline-offset-4 hover:underline" to={`/intel/incidents/${incident.id}`}>
                        {incident.title}
                      </Link>
                    </th>
                    <td className="py-2 pr-3 text-muted-foreground">{nameOf(incident.serverId)}</td>
                    <td className="py-2 pr-3">
                      <SeverityBadge severity={incident.severity} />
                    </td>
                    <td className="py-2 pr-3">{statusLabel(incident.status)}</td>
                    <td className="py-2 font-mono text-xs">{formatTs(incident.updatedAtMs)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
