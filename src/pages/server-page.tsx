import { useState } from "react";
import { DiagnosticsPanel } from "@/components/diagnostics/diagnostics-panel";
import { InfraSummaryCard } from "@/components/infra/summary-card";
import { OpsModeBanner } from "@/components/ops/operations-mode-card";
import { OpsQuickActions } from "@/components/ops/quick-actions";
import { CircleCheck, CircleDashed, CircleX, Server } from "lucide-react";
import { Link } from "react-router-dom";
import {
  formatBytesPerSec,
  formatPercent,
  MetricUplot,
} from "@/components/charts/metric-uplot";
import { EmptyState, Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Masked } from "@/contexts/privacy-context";
import { useServersContext } from "@/contexts/servers-context";
import { useTimeRange } from "@/contexts/time-range-context";
import { useCollectorBanner } from "@/hooks/use-collector-banner";
import { useServerMetrics } from "@/hooks/use-server-metrics";
import { COLLECTOR_BANNER_TEXT, plainCollectorText } from "@/lib/collector-banner";
import { cn } from "cn";

const CHART_HEIGHT = 180;

function StatPill({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex w-28 flex-col gap-1 rounded-lg border border-border bg-muted/40 px-3 py-2">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="font-mono text-lg leading-none font-medium">{value}</span>
    </div>
  );
}

function ServiceState({ active, state }: { active: boolean; state: string }) {
  const failed = state === "failed";
  const Icon = active ? CircleCheck : failed ? CircleX : CircleDashed;
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md px-2 py-0.5 text-xs font-medium",
        active
          ? "bg-status-ok/15 text-status-ok"
          : failed
            ? "bg-status-critical/15 text-status-critical"
            : "bg-muted text-muted-foreground",
      )}
    >
      <Icon className="size-3.5" aria-hidden />
      {state}
    </span>
  );
}

function CollectorBanner({
  error,
  collectorName,
  pending,
  onRetry,
}: {
  error: string;
  collectorName: string | null;
  pending: boolean;
  onRetry: () => void;
}) {
  const name = plainCollectorText(collectorName || "system");
  const detail = plainCollectorText(error);
  return (
    <Notice
      tone="warning"
      title={COLLECTOR_BANNER_TEXT}
      action={
        <Button type="button" size="sm" variant="outline" disabled={pending} onClick={onRetry}>
          {pending ? "Retrying" : "Retry"}
        </Button>
      }
    >
      <details className="text-foreground">
        <summary className="cursor-pointer rounded-sm underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
          Details
        </summary>
        <div className="mt-2 space-y-1">
          <p>Collector: {name}</p>
          <p className="break-words font-mono text-xs">{detail}</p>
        </div>
      </details>
    </Notice>
  );
}

export function ServerPage() {
  const { servers, statusById, selectedServerId, connect } = useServersContext();
  const server = servers.find((s) => s.id === selectedServerId);
  const status = selectedServerId ? statusById.get(selectedServerId) : undefined;
  const connected = status?.status === "connected" || status?.status === "degraded";
  const { isLive } = useTimeRange();
  const { snapshot, waiting } = useServerMetrics(selectedServerId, connected || !isLive);
  const banner = useCollectorBanner(isLive ? snapshot : null);
  const [retrying, setRetrying] = useState(false);

  if (!server) {
    return (
      <EmptyState
        icon={Server}
        title="No server selected"
        description="Choose a server in the sidebar or add one from Settings to view live system metrics."
        action={<Button render={<Link to="/settings" />}>Go to Settings</Button>}
      />
    );
  }

  if (isLive && !connected) {
    return (
      <EmptyState
        title={`${server.name} is not connected`}
        description="Connect from the Overview page to stream CPU, memory, disk, and network charts for this host."
        action={<Button render={<Link to="/" />}>Open Overview</Button>}
      />
    );
  }

  const loading = waiting && !snapshot;

  return (
    <div className="flex flex-col gap-6">
      <OpsModeBanner />
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div className="min-w-0">
          <h2 className="truncate text-xl font-semibold tracking-tight">{server.name}</h2>
          <p className="truncate font-mono text-sm text-muted-foreground">
            <Masked text={`${server.username}@${server.host}:${server.port}`} />
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          {snapshot ? (
            <>
              <StatPill label="CPU" value={formatPercent(snapshot.cpuPercent)} />
              <StatPill label="Memory" value={formatPercent(snapshot.memoryPercent)} />
              <StatPill label="Load (1m)" value={snapshot.load1.toFixed(2)} />
              <StatPill label="Disk" value={formatPercent(snapshot.diskPercent)} />
            </>
          ) : (
            Array.from({ length: 4 }, (_, i) => (
              <Skeleton key={i} className="h-[58px] w-28 rounded-lg" />
            ))
          )}
        </div>
      </div>

      {!isLive && (
        <Notice tone="info">
          Showing stored history. Disk I/O, network, processes and services are
          only available in Live mode.
        </Notice>
      )}
      {isLive && status?.status === "degraded" && (
        <Notice tone="warning">Connection is degraded. Updates may be delayed.</Notice>
      )}
      {banner.show && banner.error && (
        <CollectorBanner
          error={banner.error}
          collectorName={banner.collectorName}
          pending={retrying}
          onRetry={() => {
            setRetrying(true);
            void connect(server.id).finally(() => setRetrying(false));
          }}
        />
      )}

      {loading ? (
        <div className="grid gap-6 lg:grid-cols-2">
          {Array.from({ length: 8 }, (_, i) => (
            <Skeleton key={i} className="h-[256px] w-full rounded-xl" />
          ))}
        </div>
      ) : snapshot ? (
        <>
          <div className="grid gap-6 lg:grid-cols-2">
            <MetricUplot
              title="CPU"
              points={snapshot.cpuHistory}
              formatValue={formatPercent}
              color="blue"
              height={CHART_HEIGHT}
            />
            <MetricUplot
              title="Memory"
              points={snapshot.memoryHistory}
              formatValue={formatPercent}
              color="green"
              height={CHART_HEIGHT}
            />
            <MetricUplot
              title="Load average (1m)"
              label="Load 1m"
              points={snapshot.loadHistory}
              color="orange"
              height={CHART_HEIGHT}
            />
            <MetricUplot
              title="Disk usage"
              label="Used"
              points={snapshot.diskUsageHistory}
              formatValue={formatPercent}
              color="purple"
              height={CHART_HEIGHT}
            />
            {isLive && (
              <>
            <MetricUplot
              title="Disk read"
              label="Read"
              points={snapshot.diskReadHistory}
              formatValue={formatBytesPerSec}
              color="sky"
              height={CHART_HEIGHT}
            />
            <MetricUplot
              title="Disk write"
              label="Write"
              points={snapshot.diskWriteHistory}
              formatValue={formatBytesPerSec}
              color="vermillion"
              height={CHART_HEIGHT}
            />
            <MetricUplot
              title="Network in"
              label="RX"
              points={snapshot.netInHistory}
              formatValue={formatBytesPerSec}
              color="sky"
              height={CHART_HEIGHT}
            />
            <MetricUplot
              title="Network out"
              label="TX"
              points={snapshot.netOutHistory}
              formatValue={formatBytesPerSec}
              color="vermillion"
              height={CHART_HEIGHT}
            />
              </>
            )}
          </div>

          {isLive && (
          <div className="grid gap-6 lg:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Top processes</CardTitle>
              </CardHeader>
              <CardContent className="px-0">
                {snapshot.processes.length === 0 ? (
                  <p className="px-4 text-sm text-muted-foreground">
                    Waiting for process samples…
                  </p>
                ) : (
                  <div className="overflow-x-auto">
                    <table className="w-full table-fixed text-sm">
                      <caption className="sr-only">Top processes by CPU usage</caption>
                      <thead>
                        <tr className="border-b border-border text-left text-xs text-muted-foreground">
                          <th scope="col" className="w-20 px-4 py-2 font-medium">PID</th>
                          <th scope="col" className="w-24 px-2 py-2 font-medium">User</th>
                          <th scope="col" className="w-20 px-2 py-2 text-right font-medium">CPU</th>
                          <th scope="col" className="w-20 px-2 py-2 text-right font-medium">Memory</th>
                          <th scope="col" className="px-4 py-2 font-medium">Command</th>
                        </tr>
                      </thead>
                      <tbody className="font-mono">
                        {snapshot.processes.map((p) => (
                          <tr key={p.pid} className="border-b border-border/60 last:border-0">
                            <td className="px-4 py-2">{p.pid}</td>
                            <td className="truncate px-2 py-2 text-xs">{p.user}</td>
                            <td className="px-2 py-2 text-right">{p.cpuPercent.toFixed(1)}%</td>
                            <td className="px-2 py-2 text-right">{p.memoryPercent.toFixed(1)}%</td>
                            <td className="truncate px-4 py-2 text-xs" title={p.command}>
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
              <CardHeader>
                <CardTitle className="text-sm">Services</CardTitle>
                {snapshot.ssSummary.tcpEstablished != null && (
                  <CardDescription className="text-xs">
                    TCP established: {snapshot.ssSummary.tcpEstablished}
                    {snapshot.ssSummary.tcpTotal != null &&
                      ` / ${snapshot.ssSummary.tcpTotal} total`}
                  </CardDescription>
                )}
              </CardHeader>
              <CardContent>
                {snapshot.services.length === 0 ? (
                  <p className="text-sm text-muted-foreground">Waiting for systemd status…</p>
                ) : (
                  <ul className="flex flex-col gap-2">
                    {snapshot.services.map((svc) => (
                      <li
                        key={svc.name}
                        className="flex items-center justify-between gap-2 rounded-lg border border-border px-3 py-2"
                      >
                        <span className="truncate font-mono text-sm">{svc.name}</span>
                        <div className="flex flex-wrap items-center justify-end gap-2">
                          <ServiceState active={svc.active} state={svc.state} />
                          <OpsQuickActions
                            serverId={server.id}
                            kind="systemd"
                            target={svc.name}
                            includeNav={false}
                            showHint={false}
                          />
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </CardContent>
            </Card>
          </div>
          )}
        </>
      ) : (
        <EmptyState
          title="No metrics received yet"
          description="Samples arrive every few seconds after connecting."
        />
      )}
      {selectedServerId && <InfraSummaryCard serverId={selectedServerId} />}
      {selectedServerId && <DiagnosticsPanel serverId={selectedServerId} />}
    </div>
  );
}
