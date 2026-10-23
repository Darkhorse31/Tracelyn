import { Loader2, Plug, PlugZap, Server } from "lucide-react";
import { Link } from "react-router-dom";
import { Sparkline } from "@/components/charts/sparkline";
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

function borderAccent(status: ConnectionStatus | undefined) {
  switch (status) {
    case "connected":
      return "border-status-ok/40";
    case "degraded":
      return "border-status-warning/50";
    case "offline":
      return "border-status-critical/50";
    case "connecting":
      return "border-status-info/40";
    default:
      return "border-border";
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
  color: string;
  max?: number;
}) {
  return (
    <div className="flex flex-col gap-1 rounded-lg bg-muted/40 px-2 py-2">
      <div className="flex items-baseline justify-between gap-1">
        <span className="text-xs text-muted-foreground">{label}</span>
        <span className="font-mono text-xs font-medium font-tabular">{value}</span>
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
  const connectionStatus = status?.status ?? "disconnected";
  const isConnected = connectionStatus === "connected";
  const showMetrics = isConnected && metrics;

  return (
    <Card
      className={cn(
        "flex flex-col transition-[border-color,box-shadow] duration-200",
        borderAccent(connectionStatus),
        connectionStatus === "offline" &&
          "shadow-[inset_0_0_0_1px_var(--status-critical)]/10",
      )}
    >
      <CardHeader className="gap-2 pb-2">
        <div className="flex items-start justify-between gap-2">
          <div className="flex min-w-0 items-center gap-2">
            <div className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <Server />
            </div>
            <div className="min-w-0">
              <CardTitle className="truncate text-base">{server.name}</CardTitle>
              <CardDescription className="truncate font-mono text-xs">
                {server.username}@{server.host}:{server.port}
              </CardDescription>
            </div>
          </div>
          <ConnectionStatusBadge status={connectionStatus} />
        </div>
      </CardHeader>
      <CardContent className="flex flex-1 flex-col gap-3 pb-2">
        {showMetrics ? (
          <div className="grid grid-cols-2 gap-2">
            <MetricCell
              label="CPU"
              value={`${metrics.cpuPercent.toFixed(1)}%`}
              spark={metrics.cpuSparkline}
              color="var(--chart-1)"
            />
            <MetricCell
              label="Memory"
              value={`${metrics.memoryPercent.toFixed(1)}%`}
              spark={metrics.memorySparkline}
              color="var(--chart-2)"
            />
            <MetricCell
              label="Disk"
              value={`${metrics.diskPercent.toFixed(1)}%`}
              spark={metrics.diskSparkline}
              color="var(--chart-4)"
            />
            <MetricCell
              label="Load"
              value={metrics.load1.toFixed(2)}
              spark={metrics.loadSparkline}
              max={Math.max(4, metrics.load1 * 1.5, 1)}
              color="var(--chart-3)"
            />
          </div>
        ) : busy && connectionStatus === "connecting" ? (
          <div className="flex flex-col gap-2">
            <Skeleton className="h-4 w-3/4" />
            <Skeleton className="h-16 w-full" />
          </div>
        ) : status?.osInfo ? (
          <p className="line-clamp-2 font-mono text-xs text-muted-foreground">
            {status.osInfo}
          </p>
        ) : (
          <p className="text-sm text-muted-foreground">
            {connectionStatus === "offline" && status?.lastError
              ? status.lastError
              : "Connect to fetch system information and begin monitoring."}
          </p>
        )}
        {status?.hostname && !showMetrics && (
          <p className="font-mono text-xs text-muted-foreground font-tabular">
            Hostname: {status.hostname}
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
      <CardFooter className="gap-2 pt-2">
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
