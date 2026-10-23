import { useMemo, useState } from "react";
import { AlertCircle } from "lucide-react";
import { MetricCard } from "@/components/web/metric-card";
import { SparklineChart } from "@/components/web/sparkline-chart";
import { StatusStackBar } from "@/components/web/status-stack-bar";
import { VirtualLogList } from "@/components/web/virtual-log-list";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { useServersContext } from "@/contexts/servers-context";
import { filterWebLogs, useWebTraffic } from "@/hooks/use-web-traffic";

export function WebTrafficPage() {
  const { selectedServerId } = useServersContext();
  const { metrics, logs, loading, permissionDenied, webDetected } =
    useWebTraffic(selectedServerId);
  const [pathQuery, setPathQuery] = useState("");
  const [ipQuery, setIpQuery] = useState("");
  const [textQuery, setTextQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<string>("all");

  const filtered = useMemo(
    () =>
      filterWebLogs(logs, {
        pathQuery,
        ipQuery,
        textQuery,
        statusMin: statusFilter === "5xx" ? 500 : undefined,
        statusMax:
          statusFilter === "4xx"
            ? 499
            : statusFilter === "5xx"
              ? undefined
              : statusFilter === "2xx"
                ? 299
                : undefined,
      }),
    [logs, pathQuery, ipQuery, textQuery, statusFilter],
  );

  if (!selectedServerId) {
    return (
      <div className="rounded-xl border border-dashed border-border bg-card/40 px-6 py-12 text-center text-sm text-muted-foreground">
        Select a server in the sidebar and connect to view live web traffic.
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      {permissionDenied && (
        <div
          className="flex gap-3 rounded-lg border border-status-critical/40 bg-status-critical/10 px-4 py-3 text-sm"
          role="alert"
        >
          <AlertCircle className="mt-0.5 size-4 shrink-0 text-status-critical" />
          <p>
            Cannot read the access log on the server (permission denied). Ensure
            the SSH user can read the web server log files, or use a group such
            as <span className="font-mono text-xs">adm</span> on Debian/Ubuntu.
          </p>
        </div>
      )}

      {!webDetected && !permissionDenied && (
        <div className="rounded-lg border border-border bg-muted/30 px-4 py-3 text-sm text-muted-foreground">
          No nginx or Apache process detected on this host. Web traffic metrics
          will stay empty until a supported web server is running.
        </div>
      )}

      {metrics.timingNotice && (
        <div
          className="flex gap-3 rounded-lg border border-status-warning/40 bg-status-warning/10 px-4 py-3 text-sm"
          role="status"
        >
          <AlertCircle className="mt-0.5 size-4 shrink-0 text-status-warning" />
          <div className="min-w-0 flex-1">
            <p>{metrics.timingNotice.message}</p>
            <pre className="mt-2 overflow-x-auto rounded-md bg-background/60 p-2 font-mono text-xs">
              {metrics.timingNotice.suggestedLine}
            </pre>
          </div>
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <MetricCard
          title="Requests / sec"
          value={metrics.reqPerSec.toFixed(1)}
          loading={loading}
        />
        <MetricCard
          title="Bandwidth"
          value={`${metrics.bandwidthMbps.toFixed(1)} Mbps`}
          loading={loading}
        />
        <Card className="sm:col-span-2">
          <CardHeader className="pb-1">
            <CardTitle className="text-xs font-medium text-muted-foreground">
              Req/s (live)
            </CardTitle>
          </CardHeader>
          <CardContent>
            {loading ? (
              <Skeleton className="h-12 w-full" />
            ) : (
              <SparklineChart
                data={metrics.reqPerSecSeries}
                className="h-12 w-full"
              />
            )}
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Status codes</CardTitle>
          </CardHeader>
          <CardContent>
            {loading ? (
              <Skeleton className="h-10 w-full" />
            ) : (
              <StatusStackBar breakdown={metrics.statusBreakdown} />
            )}
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Top URLs &amp; IPs</CardTitle>
          </CardHeader>
          <CardContent className="grid gap-4 sm:grid-cols-2">
            <div>
              <p className="mb-2 text-xs text-muted-foreground">URLs</p>
              <ul className="space-y-1 font-mono text-xs">
                {metrics.topUrls.map((u) => (
                  <li key={u.path} className="flex justify-between gap-2">
                    <span className="truncate">{u.path}</span>
                    <span className="tabular-nums text-muted-foreground">
                      {u.count}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
            <div>
              <p className="mb-2 text-xs text-muted-foreground">IPs</p>
              <ul className="space-y-1 font-mono text-xs">
                {metrics.topIps.map((u) => (
                  <li key={u.ip} className="flex justify-between gap-2">
                    <span>{u.ip}</span>
                    <span className="tabular-nums text-muted-foreground">
                      {u.count}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Live access log</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="grid gap-2 sm:grid-cols-4">
            <Input
              placeholder="Filter path"
              value={pathQuery}
              onChange={(e) => setPathQuery(e.target.value)}
              className="font-mono text-xs"
            />
            <Input
              placeholder="Filter IP"
              value={ipQuery}
              onChange={(e) => setIpQuery(e.target.value)}
              className="font-mono text-xs"
            />
            <Input
              placeholder="Search raw line"
              value={textQuery}
              onChange={(e) => setTextQuery(e.target.value)}
              className="font-mono text-xs sm:col-span-2"
            />
          </div>
          <div className="flex flex-wrap gap-2 text-xs">
            {(["all", "2xx", "4xx", "5xx"] as const).map((f) => (
              <button
                key={f}
                type="button"
                className={`rounded-md border px-2 py-1 transition-colors duration-150 ${
                  statusFilter === f
                    ? "border-primary bg-primary/15 text-foreground"
                    : "border-border text-muted-foreground hover:bg-muted/50"
                }`}
                onClick={() => setStatusFilter(f)}
              >
                {f}
              </button>
            ))}
            <span className="ml-auto self-center font-mono tabular-nums text-muted-foreground">
              {filtered.length} lines
            </span>
          </div>
          <VirtualLogList lines={filtered} />
        </CardContent>
      </Card>
    </div>
  );
}
