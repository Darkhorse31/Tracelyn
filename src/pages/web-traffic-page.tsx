import { useMemo } from "react";
import { useRememberedState } from "@/hooks/use-remembered-state";
import { Globe } from "lucide-react";
import { toast } from "sonner";
import { formatBytesPerSec } from "@/components/charts/metric-uplot";
import { HowToFix } from "@/components/setup/how-to-fix";
import { EmptyState, Notice } from "@/components/status/notice";
import { MetricCard } from "@/components/web/metric-card";
import { SparklineChart } from "@/components/web/sparkline-chart";
import { StatusStackBar } from "@/components/web/status-stack-bar";
import { VirtualLogList } from "@/components/web/virtual-log-list";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { usePrivacy } from "@/contexts/privacy-context";
import { useServersContext } from "@/contexts/servers-context";
import { useTimeRange } from "@/contexts/time-range-context";
import { filterWebLogs, useWebTraffic } from "@/hooks/use-web-traffic";
import { cn } from "cn";

const STATUS_FILTERS = {
  all: [undefined, undefined],
  "2xx": [200, 299],
  "3xx": [300, 399],
  "4xx": [400, 499],
  "5xx": [500, 599],
} as const;

type StatusFilter = keyof typeof STATUS_FILTERS;

function TopList({
  title,
  rows,
}: {
  title: string;
  rows: { key: string; count: number }[];
}) {
  const { enabled, mask } = usePrivacy();
  return (
    <div className="min-w-0">
      <h3 className="mb-2 text-xs font-medium text-muted-foreground">{title}</h3>
      {rows.length === 0 ? (
        <p className="text-xs text-muted-foreground">No requests yet.</p>
      ) : (
        <table className="w-full table-fixed font-mono text-xs">
          <caption className="sr-only">{title} by request count</caption>
          <thead className="sr-only">
            <tr>
              <th scope="col">{title}</th>
              <th scope="col">Requests</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.key}>
                <td className={cn("truncate py-0.5 pr-2", enabled && "privacy-blur")} title={mask(r.key)} data-privacy-mask={enabled ? "" : undefined}>
                  {mask(r.key)}
                </td>
                <td className="w-16 py-0.5 text-right text-muted-foreground">
                  {r.count.toLocaleString()}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

export function WebTrafficPage() {
  const { selectedServerId, servers, statusById } = useServersContext();
  const { isLive } = useTimeRange();
  const { metrics, logs, loading, permissionDenied, webDetected } =
    useWebTraffic(selectedServerId);
  const scope = `web:${selectedServerId ?? "none"}`;
  const [pathQuery, setPathQuery] = useRememberedState(`${scope}:path`, "");
  const [ipQuery, setIpQuery] = useRememberedState(`${scope}:ip`, "");
  const [textQuery, setTextQuery] = useRememberedState(`${scope}:text`, "");
  const [statusFilter, setStatusFilter] = useRememberedState<StatusFilter>(`${scope}:status`, "all");

  const filtered = useMemo(() => {
    const [statusMin, statusMax] = STATUS_FILTERS[statusFilter] ?? STATUS_FILTERS.all;
    return filterWebLogs(logs, { pathQuery, ipQuery, textQuery, statusMin, statusMax });
  }, [logs, pathQuery, ipQuery, textQuery, statusFilter]);

  const server = servers.find((s) => s.id === selectedServerId);
  const connection = selectedServerId ? statusById.get(selectedServerId)?.status : undefined;
  const connected = connection === "connected" || connection === "degraded";

  if (!server) {
    return (
      <EmptyState
        icon={Globe}
        title="No server selected"
        description="Select a server in the sidebar and connect to view live web traffic."
      />
    );
  }

  if (isLive && !connected) {
    return (
      <EmptyState
        icon={Globe}
        title={`${server.name} is not connected`}
        description="Connect the server from the Overview page to stream its access log."
      />
    );
  }

  const total = Object.values(metrics.statusBreakdown).reduce((a, b) => a + b, 0);
  const share5xx = total ? ((metrics.statusBreakdown["5xx"] ?? 0) / total) * 100 : 0;

  return (
    <div className="flex flex-col gap-6">
      {!isLive && (
        <Notice tone="info">
          Showing stored request-rate history. Status codes, top lists and the access log
          are only available in Live mode.
        </Notice>
      )}
      {isLive && permissionDenied && (
        <Notice
          tone="critical"
          title="web-access: access logs not readable"
          action={<HowToFix text="web-access: access logs not readable" />}
        >
          Make sure the SSH user can read the web server log files, for example via the{" "}
          <span className="font-mono text-xs">adm</span> group on Debian/Ubuntu.
        </Notice>
      )}
      {isLive && !loading && !webDetected && !permissionDenied && (
        <Notice tone="info">
          No nginx or Apache process detected on this host. Web traffic metrics stay empty
          until a supported web server is running.
        </Notice>
      )}
      {metrics.timingNotice && (
        <Notice
          tone="warning"
          title={metrics.timingNotice.message}
          action={
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => {
                void navigator.clipboard.writeText(metrics.timingNotice?.suggestedLine ?? "");
                toast.success("Copied log_format");
              }}
            >
              Copy
            </Button>
          }
        >
          <pre className="mt-2 overflow-x-auto rounded-md bg-background/60 p-2 font-mono text-xs">
            {metrics.timingNotice.suggestedLine}
          </pre>
        </Notice>
      )}

      <div className="grid gap-6 sm:grid-cols-2 xl:grid-cols-4">
        <MetricCard
          title="Requests / sec"
          value={metrics.reqPerSec.toFixed(1)}
          loading={loading}
        />
        <MetricCard
          title="Bandwidth"
          value={isLive ? formatBytesPerSec((metrics.bandwidthMbps * 1_000_000) / 8) : "—"}
          loading={loading}
        />
        <MetricCard
          title="5xx share"
          value={isLive ? `${share5xx.toFixed(2)}%` : "—"}
          loading={loading}
        />
        <MetricCard
          title={isLive ? "Requests / sec (live)" : "Requests / sec (range)"}
          loading={loading}
          value={
            <SparklineChart
              data={metrics.reqPerSecSeries}
              label="Requests per second"
              height={32}
              className="h-8 w-full"
            />
          }
        />
      </div>

      {isLive && (
        <>
          <div className="grid gap-6 lg:grid-cols-2">
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
              <CardContent className="grid gap-6 sm:grid-cols-2">
                <TopList
                  title="URLs"
                  rows={metrics.topUrls.map((u) => ({ key: u.path, count: u.count }))}
                />
                <TopList
                  title="Client IPs"
                  rows={metrics.topIps.map((u) => ({ key: u.ip, count: u.count }))}
                />
              </CardContent>
            </Card>
          </div>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Live access log</CardTitle>
            </CardHeader>
            <CardContent className="flex flex-col gap-4">
              <div className="grid gap-2 sm:grid-cols-4">
                <Input
                  aria-label="Filter by path"
                  placeholder="Filter path"
                  value={pathQuery}
                  onChange={(e) => setPathQuery(e.target.value)}
                  className="font-mono text-xs"
                />
                <Input
                  aria-label="Filter by client IP"
                  placeholder="Filter IP"
                  value={ipQuery}
                  onChange={(e) => setIpQuery(e.target.value)}
                  className="font-mono text-xs"
                />
                <Input
                  aria-label="Search raw log line"
                  placeholder="Search raw line"
                  value={textQuery}
                  onChange={(e) => setTextQuery(e.target.value)}
                  className="font-mono text-xs sm:col-span-2"
                />
              </div>
              <div className="flex flex-wrap items-center gap-2 text-xs">
                <div role="group" aria-label="Status code filter" className="flex gap-1">
                  {(Object.keys(STATUS_FILTERS) as StatusFilter[]).map((f) => (
                    <button
                      key={f}
                      type="button"
                      aria-pressed={statusFilter === f}
                      className={cn(
                        "h-7 min-w-11 rounded-md border px-2 font-mono transition-colors duration-150",
                        statusFilter === f
                          ? "border-primary bg-primary/15 font-medium text-foreground"
                          : "border-border text-muted-foreground hover:bg-muted",
                      )}
                      onClick={() => setStatusFilter(f)}
                    >
                      {f}
                    </button>
                  ))}
                </div>
                <span className="ml-auto font-mono text-muted-foreground">
                  {filtered.length.toLocaleString()} of {logs.length.toLocaleString()} lines
                </span>
              </div>
              <VirtualLogList lines={filtered} />
            </CardContent>
          </Card>
        </>
      )}
    </div>
  );
}
