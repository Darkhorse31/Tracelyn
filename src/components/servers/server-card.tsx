import { Loader2, Plug, PlugZap, Server } from "lucide-react";
import { Link } from "react-router-dom";
import { Sparkline } from "@/components/charts/sparkline";
import type { ChartColor } from "@/lib/chart-theme";
import { ConnectionStatusBadge } from "@/components/status/connection-status-badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Masked, usePrivacy } from "@/contexts/privacy-context";
import type {
  ConnectionStatus,
  OverviewServerMetrics,
  ServerRecord,
  ServerStatus,
} from "@/lib/types";
import { cn } from "cn";

interface ServerCardProps {
  server: ServerRecord;
  status?: ServerStatus;
  metrics?: OverviewServerMetrics;
  onConnect: () => void;
  onDisconnect: () => void;
  busy?: boolean;
}

/** Unhealthy servers get a colored ring; the badge carries the text and icon. */
function ringAccent(status: ConnectionStatus | undefined) {
  switch (status) {
    case "degraded":
      return "ring-2 ring-status-warning/70";
    case "offline":
      return "ring-2 ring-status-critical/70";
    default:
      return "";
  }
}

function MetricCell({
  label,
  value,
  spark,
  color,
  max = 100,
}: {
  label: string;
  value: string;
  spark: number[];
  color: ChartColor;
  max?: number;
}) {
  return (
    <div className="flex flex-col gap-2 rounded-lg bg-muted/50 p-2">
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-xs text-muted-foreground">{label}</span>
        <span className="font-mono text-sm font-medium">{value}</span>
      </div>
      <Sparkline values={spark} color={color} max={max} />
    </div>
  );
}

export function ServerCard({
  server,
  status,
  metrics,
  onConnect,
  onDisconnect,
  busy,
}: ServerCardProps) {
  const { enabled, mask } = usePrivacy();
  const connectionStatus = status?.status ?? "disconnected";
  const isConnected = connectionStatus === "connected";
  const showMetrics = isConnected && metrics;

  return (
    <Card
      role="group"
      aria-label={enabled ? "Monitored server" : mask(server.name)}
      className={cn(
        "flex flex-col transition-shadow duration-200",
        ringAccent(connectionStatus),
      )}
    >
      <CardHeader>
        <div className="flex items-start justify-between gap-2">
          <div className="flex min-w-0 items-center gap-2">
            <div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <Server className="size-4" aria-hidden />
            </div>
            <div className="min-w-0">
              <CardTitle className="truncate text-base">{enabled ? "Monitored server" : mask(server.name)}</CardTitle>
              <CardDescription className="truncate font-mono text-xs">
                <Masked text={`${server.username}@${server.host}:${server.port}`} />
              </CardDescription>
            </div>
          </div>
          <ConnectionStatusBadge status={connectionStatus} />
        </div>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col gap-4">
        {showMetrics ? (
          <div className="grid grid-cols-2 gap-2">
            <MetricCell
              label="CPU"
              value={`${metrics.cpuPercent.toFixed(1)}%`}
              spark={metrics.cpuSparkline}
              color="blue"
            />
            <MetricCell
              label="Memory"
              value={`${metrics.memoryPercent.toFixed(1)}%`}
              spark={metrics.memorySparkline}
              color="green"
            />
            <MetricCell
              label="Disk"
              value={`${metrics.diskPercent.toFixed(1)}%`}
              spark={metrics.diskSparkline}
              color="purple"
            />
            <MetricCell
              label="Load"
              value={metrics.load1.toFixed(2)}
              spark={metrics.loadSparkline}
              max={Math.max(4, metrics.load1 * 1.5, 1)}
              color="orange"
            />
          </div>
        ) : connectionStatus === "connecting" || (isConnected && !metrics) ? (
          <div className="grid grid-cols-2 gap-2" aria-busy="true" aria-label="Loading metrics">
            {Array.from({ length: 4 }, (_, i) => (
              <Skeleton key={i} className="h-[76px] rounded-lg" />
            ))}
          </div>
        ) : status?.osInfo ? (
          <p className="line-clamp-2 font-mono text-xs text-muted-foreground">
            {enabled ? "System details hidden" : mask(status.osInfo)}
          </p>
        ) : (
          <p className="text-sm text-muted-foreground">
            {connectionStatus === "offline" && status?.lastError
              ? "We could not reach this server. Check its connection and hosting status."
              : "Connect to fetch system information and begin monitoring."}
          </p>
        )}
        {status?.hostname && !showMetrics && (
          <p className="font-mono text-xs text-muted-foreground">
            Hostname: {enabled ? "Hidden" : mask(status.hostname)}
          </p>
        )}
        {server.labels.length > 0 && (
          <div className="flex flex-wrap gap-1">
            {server.labels.map((label) => (
              <span
                key={label}
                className="rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground"
              >
                {label}
              </span>
            ))}
          </div>
        )}
      </CardContent>
      <CardFooter className="gap-2">
        {isConnected ? (
          <>
            <Button variant="outline" onClick={onDisconnect} disabled={busy}>
              <Plug data-icon="inline-start" />
              Disconnect
            </Button>
            <Button variant="secondary" render={<Link to="/server" />}>
              Details
            </Button>
          </>
        ) : (
          <Button onClick={onConnect} disabled={busy}>
            {busy ? (
              <Loader2 className="animate-spin" data-icon="inline-start" />
            ) : (
              <PlugZap data-icon="inline-start" />
            )}
            Connect
          </Button>
        )}
      </CardFooter>
    </Card>
  );
}
