import { AlertCircle, Server } from "lucide-react";
import { Link } from "react-router-dom";
import {
  formatBytesPerSec,
  formatPercent,
  MetricUplot,
} from "@/components/charts/metric-uplot";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useServersContext } from "@/contexts/servers-context";
import { useServerMetrics } from "@/hooks/use-server-metrics";
import { cn } from "cn";

function StatPill({
  label,
  value,
  unit,
}: {
  label: string;
  value: string;
  unit?: string;
}) {
  return (
    <div className="flex flex-col gap-0.5 rounded-lg border border-border bg-muted/30 px-3 py-2">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="font-mono text-lg font-medium font-tabular leading-none">
        {value}
        {unit && (
          <span className="ml-1 text-sm font-normal text-muted-foreground">
            {unit}
          </span>
        )}
      </span>
    </div>
  );
}

export function ServerPage() {
  const { servers, statusById, selectedServerId } = useServersContext();
  const server = servers.find((s) => s.id === selectedServerId);
  const status = selectedServerId
    ? statusById.get(selectedServerId)
    : undefined;
  const connected = status?.status === "connected";
  const { snapshot, waiting } = useServerMetrics(
    selectedServerId,
    connected,
  );

  if (!server) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-4 rounded-xl border border-dashed border-border bg-card/40 px-6 py-16 text-center">
        <Server className="size-10 text-muted-foreground" />
        <h2 className="text-lg font-semibold">No server selected</h2>
        <p className="max-w-md text-sm text-muted-foreground">
          Choose a server in the sidebar or add one from Settings to view live
          system metrics.
        </p>
        <Button render={<Link to="/settings" />}>Go to Settings</Button>
      </div>
    );
  }

  if (!connected) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-4 rounded-xl border border-border bg-card/50 px-6 py-16 text-center">
        <AlertCircle className="size-10 text-status-warning" />
        <h2 className="text-lg font-semibold">{server.name}</h2>
        <p className="max-w-md text-sm text-muted-foreground">
          Connect from the Overview page to stream CPU, memory, disk, and network
          charts for this host.
        </p>
        <Button render={<Link to="/" />}>Open Overview</Button>
      </div>
    );
  }

  const loading = waiting && !snapshot;

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h2 className="text-xl font-semibold tracking-tight">{server.name}</h2>
          <p className="font-mono text-sm text-muted-foreground">
            {server.username}@{server.host}:{server.port}
          </p>
        </div>
        {snapshot && (
          <div className="flex flex-wrap gap-2">
            <StatPill
              label="CPU"
              value={formatPercent(snapshot.cpuPercent)}
            />
            <StatPill
              label="Memory"
              value={formatPercent(snapshot.memoryPercent)}
            />
            <StatPill
              label="Load (1m)"
              value={snapshot.load1.toFixed(2)}
            />
            <StatPill
              label="Disk"
              value={formatPercent(snapshot.diskPercent)}
            />
          </div>
        )}
      </div>

      {snapshot?.collectorError && (
        <div className="rounded-lg border border-status-warning/40 bg-status-warning/10 px-4 py-3 text-sm text-status-warning">
          Collector warning: {snapshot.collectorError}
        </div>
      )}

      {loading ? (
        <div className="grid gap-4 lg:grid-cols-2">
          {Array.from({ length: 6 }).map((_, i) => (
            <Skeleton key={i} className="h-[220px] w-full rounded-xl" />
          ))}
        </div>
      ) : snapshot ? (
        <>
          <div className="grid gap-4 lg:grid-cols-2">
            <MetricUplot
              title="CPU"
              points={snapshot.cpuHistory}
              formatValue={formatPercent}
              color="var(--chart-1)"
            />
            <MetricUplot
              title="Memory"
              points={snapshot.memoryHistory}
              formatValue={formatPercent}
              color="var(--chart-2)"
            />
            <MetricUplot
              title="Load average (1m)"
              points={snapshot.loadHistory}
              color="var(--chart-3)"
            />
            <MetricUplot
              title="Disk usage"
              points={snapshot.diskUsageHistory}
              formatValue={formatPercent}
              color="var(--chart-4)"
            />
            <MetricUplot
              title="Disk read"
              points={snapshot.diskReadHistory}
              formatValue={formatBytesPerSec}
              color="var(--chart-1)"
            />
            <MetricUplot
              title="Disk write"
              points={snapshot.diskWriteHistory}
              formatValue={formatBytesPerSec}
              color="var(--chart-2)"
            />
            <MetricUplot
              title="Network in"
              points={snapshot.netInHistory}
              formatValue={formatBytesPerSec}
              color="var(--chart-3)"
            />
            <MetricUplot
              title="Network out"
              points={snapshot.netOutHistory}
              formatValue={formatBytesPerSec}
              color="var(--chart-4)"
            />
          </div>

          <div className="grid gap-4 lg:grid-cols-2">
            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-base">Top processes</CardTitle>
              </CardHeader>
              <CardContent className="p-0">
                {snapshot.processes.length === 0 ? (
                  <p className="px-4 pb-4 text-sm text-muted-foreground">
                    Waiting for process samples…
                  </p>
                ) : (
                  <div className="overflow-x-auto">
                    <table className="w-full text-sm">
                      <thead>
                        <tr className="border-b border-border text-left text-xs text-muted-foreground">
                          <th className="px-4 py-2 font-medium">PID</th>
                          <th className="px-2 py-2 font-medium">User</th>
                          <th className="px-2 py-2 font-medium text-right">
                            CPU
                          </th>
                          <th className="px-2 py-2 font-medium text-right">
                            Mem
                          </th>
                          <th className="px-4 py-2 font-medium">Command</th>
                        </tr>
                      </thead>
                      <tbody>
                        {snapshot.processes.map((p) => (
                          <tr
                            key={p.pid}
                            className="border-b border-border/60 last:border-0"
                          >
                            <td className="px-4 py-2 font-mono font-tabular">
                              {p.pid}
                            </td>
                            <td className="px-2 py-2 font-mono text-xs">
                              {p.user}
                            </td>
                            <td className="px-2 py-2 text-right font-mono font-tabular">
                              {p.cpuPercent.toFixed(1)}%
                            </td>
                            <td className="px-2 py-2 text-right font-mono font-tabular">
                              {p.memoryPercent.toFixed(1)}%
                            </td>
                            <td className="max-w-[200px] truncate px-4 py-2 font-mono text-xs">
                              {p.command}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </CardContent>
            </Card>

            <Card>
              <CardHeader className="pb-2">
                <CardTitle className="text-base">Services</CardTitle>
                {snapshot.ssSummary.tcpEstablished != null && (
                  <p className="text-xs text-muted-foreground font-tabular">
                    TCP established: {snapshot.ssSummary.tcpEstablished}
                    {snapshot.ssSummary.tcpTotal != null &&
                      ` / ${snapshot.ssSummary.tcpTotal} total`}
                  </p>
                )}
              </CardHeader>
              <CardContent>
                {snapshot.services.length === 0 ? (
                  <p className="text-sm text-muted-foreground">
                    Waiting for systemd status…
                  </p>
                ) : (
                  <ul className="flex flex-col gap-2">
                    {snapshot.services.map((svc) => (
                      <li
                        key={svc.name}
                        className="flex items-center justify-between gap-2 rounded-lg border border-border/80 px-3 py-2"
                      >
                        <span className="font-mono text-sm">{svc.name}</span>
                        <span
                          className={cn(
                            "rounded-md px-2 py-0.5 text-xs font-medium font-tabular",
                            svc.active
                              ? "bg-status-ok/15 text-status-ok"
                              : svc.state === "failed"
                                ? "bg-status-critical/15 text-status-critical"
                                : "bg-muted text-muted-foreground",
                          )}
                        >
                          {svc.state}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </CardContent>
            </Card>
          </div>
        </>
      ) : (
        <div className="rounded-xl border border-dashed border-border px-6 py-12 text-center text-sm text-muted-foreground">
          No metrics received yet. Samples arrive every few seconds after connect.
        </div>
      )}
    </div>
  );
}
