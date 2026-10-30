import { WhatChanged } from "@/components/charts/baseline-insights"
import { CompanionCard } from "@/components/companion/companion-card";
import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { DiagnosticsPanel } from "@/components/diagnostics/diagnostics-panel";
import { InfraSummaryCard } from "@/components/infra/summary-card";
import { OpsModeBanner } from "@/components/ops/operations-mode-card";
import { OpsQuickActions } from "@/components/ops/quick-actions";
import { CircleCheck, CircleDashed, CircleX, Server } from "lucide-react";
import {
  formatBytesPerSec,
  formatPercent,
  MetricUplot,
} from "@/components/charts/metric-uplot";
import { EmptyState, Notice } from "@/components/status/notice";
import { AroundSheet } from "@/components/correlation/around-sheet";
import { PageHeader } from "@/components/layout/page-header";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Masked } from "@/contexts/privacy-context";
import { useServersContext } from "@/contexts/servers-context";
import { useTimeRange } from "@/contexts/time-range-context";
import { useCollectorBanner } from "@/hooks/use-collector-banner";
import { useLiveRing, type LiveSample } from "@/hooks/use-live-ring";
import { useServerMetrics } from "@/hooks/use-server-metrics";
import { useRememberedState } from "@/hooks/use-remembered-state";
import { setLiveMetrics } from "@/lib/api";
import type { MetricPoint } from "@/lib/types";
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
  const { range, isLive, setCursorMs } = useTimeRange();
  const { snapshot, waiting } = useServerMetrics(selectedServerId, connected || !isLive);
  const banner = useCollectorBanner(isLive ? snapshot : null);
  const [retrying, setRetrying] = useState(false);
  const [live1s, setLive1s] = useRememberedState("server:live-1s", false);
  const [paused, setPaused] = useState(false);
  const [scrub, setScrub] = useState(0);
  const [around, setAround] = useState<number | null>(null);
  const samples = useLiveRing(isLive ? snapshot : null, live1s && isLive);
  const end = paused ? Math.min(scrub, Math.max(samples.length - 1, 0)) : Math.max(samples.length - 1, 0);
  const view = samples.slice(0, end + 1).slice(-600);
  const useRing = live1s && isLive && view.length >= 2;

  useEffect(() => {
    if (!selectedServerId) return;
    void setLiveMetrics(selectedServerId, Boolean(live1s && connected && isLive));
    return () => {
      void setLiveMetrics(selectedServerId, false);
    };
  }, [selectedServerId, live1s, connected, isLive]);

  const openAt = (ts: number) => {
    setCursorMs(ts);
    setAround(ts);
  };
  const series = (pick: (sample: LiveSample) => number, fallback: MetricPoint[]) =>
    useRing ? view.map((sample) => ({ t: sample.t, v: pick(sample) })) : fallback;

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
      <PageHeader
        title={server.name}
        description={
          <span className="truncate font-mono">
            <Masked text={`${server.username}@${server.host}:${server.port}`} />
          </span>
        }
        actions={
          <>
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
          </>
        }
      />
      <CompanionCard serverId={selectedServerId} />

      {isLive && connected && (
        <div className="flex flex-wrap items-center gap-4">
          <div className="flex h-(--control-height) items-center gap-2">
            <Switch id="live-1s" checked={live1s} onCheckedChange={(value) => setLive1s(Boolean(value))} />
            <Label htmlFor="live-1s">Live 1s</Label>
          </div>
          <Button
            type="button"
            variant={paused ? "secondary" : "outline"}
            aria-pressed={paused}
            disabled={!live1s}
            onClick={() => {
              setPaused((value) => {
                const next = !value;
                if (next) setScrub(Math.max(samples.length - 1, 0));
                return next;
              });
            }}
          >
            {paused ? "Resume" : "Pause"}
          </Button>
          <label className="flex h-(--control-height) min-w-48 flex-1 items-center gap-2 text-sm text-muted-foreground">
            <span className="shrink-0">Scrub</span>
            <input
              type="range"
              className="h-2 w-full accent-primary"
              min={0}
              max={Math.max(samples.length - 1, 0)}
              value={end}
              disabled={!paused || samples.length < 2}
              aria-label="Scrub back through the last 10 minutes"
              onChange={(event) => setScrub(Number(event.target.value))}
            />
          </label>
          <Button variant="outline" render={<Link to="/processes" />}>
            All processes
          </Button>
        </div>
      )}

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
        <div className="grid items-stretch gap-4 lg:grid-cols-2">
          {Array.from({ length: 8 }, (_, i) => (
            <Skeleton key={i} className="h-[256px] w-full rounded-xl" />
          ))}
        </div>
      ) : snapshot ? (
        <>
          <WhatChanged serverId={server.id} start={Math.max(range.startMs,range.endMs-86400000)} end={range.endMs} />
          <div className="grid items-stretch gap-4 lg:grid-cols-2">
            <MetricUplot
              title="CPU"
              baselineServerId={server.id} metricKey="cpu.percent"
              points={series((sample) => sample.cpu, snapshot.cpuHistory)}
              formatValue={formatPercent}
              color="blue"
              height={CHART_HEIGHT}
              onPointClick={openAt}
            />
            <MetricUplot
              title="Memory"
              baselineServerId={server.id} metricKey="memory.percent"
              points={series((sample) => sample.mem, snapshot.memoryHistory)}
              formatValue={formatPercent}
              color="green"
              height={CHART_HEIGHT}
              onPointClick={openAt}
            />
            <MetricUplot
              title="Load average (1m)"
              baselineServerId={server.id} metricKey="load.1"
              label="Load 1m"
              points={series((sample) => sample.load, snapshot.loadHistory)}
              color="orange"
              height={CHART_HEIGHT}
              onPointClick={openAt}
            />
            <MetricUplot
              title="Disk usage"
              baselineServerId={server.id} metricKey="disk.percent"
              label="Used"
              points={series((sample) => sample.disk, snapshot.diskUsageHistory)}
              formatValue={formatPercent}
              color="purple"
              height={CHART_HEIGHT}
              onPointClick={openAt}
            />
            {isLive && (
              <>
            <MetricUplot
              title="Disk read"
              label="Read"
              points={series((sample) => sample.diskRead, snapshot.diskReadHistory)}
              formatValue={formatBytesPerSec}
              color="sky"
              height={CHART_HEIGHT}
              onPointClick={openAt}
            />
            <MetricUplot
              title="Disk write"
              label="Write"
              points={series((sample) => sample.diskWrite, snapshot.diskWriteHistory)}
              formatValue={formatBytesPerSec}
              color="vermillion"
              height={CHART_HEIGHT}
              onPointClick={openAt}
            />
            <MetricUplot
              title="Network in"
              label="RX"
              points={series((sample) => sample.netIn, snapshot.netInHistory)}
              formatValue={formatBytesPerSec}
              color="sky"
              height={CHART_HEIGHT}
              onPointClick={openAt}
            />
            <MetricUplot
              title="Network out"
              label="TX"
              points={series((sample) => sample.netOut, snapshot.netOutHistory)}
              formatValue={formatBytesPerSec}
              color="vermillion"
              height={CHART_HEIGHT}
              onPointClick={openAt}
            />
              </>
            )}
          </div>

          {isLive && (
          <div className="grid items-stretch gap-4 lg:grid-cols-2">
            <Card className="h-full">
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

            <Card className="h-full">
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
      {selectedServerId && (
        <AroundSheet serverId={selectedServerId} timestampMs={around} onClose={() => setAround(null)} />
      )}
      {selectedServerId && <InfraSummaryCard serverId={selectedServerId} />}
      {selectedServerId && <DiagnosticsPanel serverId={selectedServerId} />}
    </div>
  );
}
