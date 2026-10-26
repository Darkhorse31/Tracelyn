import { useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { OpsQuickActions } from "@/components/ops/quick-actions";
import type { OpsKind } from "@/lib/ops-types";
import { Boxes } from "lucide-react";
import { MetricUplot } from "@/components/charts/metric-uplot";
import { ErrorIssues } from "@/components/insight/error-issues";
import { OtelSetup } from "@/components/insight/otel-setup";
import { LogExplorer } from "@/components/logs/log-explorer";
import { TraceBrowser } from "@/components/traces/trace-browser";
import { HowToFix } from "@/components/setup/how-to-fix";
import { EmptyState } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useApps } from "@/hooks/use-apps";
import { cn } from "@/lib/utils";

const TABS = ["Overview", "Logs", "Requests", "Errors", "Traces"] as const;

function opsKind(kind: string): OpsKind | null {
  if (kind === "pm2" || kind === "systemd" || kind === "docker" || kind === "nginx" || kind === "apache") {
    return kind;
  }
  return null;
}

export function AppDetailPage() {
  const { appId } = useParams();
  const [params] = useSearchParams();
  const { apps, history, loading, serverId } = useApps();
  const app = apps.find((row) => row.id === appId);
  const requested = params.get("tab");
  const [tab, setTab] = useState<(typeof TABS)[number]>(
    TABS.find((name) => name.toLowerCase() === (requested ?? "").toLowerCase()) ?? "Overview",
  );

  if (!serverId) {
    return (
      <EmptyState
        icon={Boxes}
        title="No server selected"
        description="Select a server, then open an application."
      />
    );
  }

  if (loading && !app) return <Skeleton className="h-64 w-full" />;

  if (!app) {
    return (
      <EmptyState
        icon={Boxes}
        title="Application not found"
        description="It may have been hidden or the server has not reported it yet."
        action={<Button render={<Link to="/apps" />}>Back to applications</Button>}
      />
    );
  }

  const series = history[app.id];

  return (
    <div className="flex flex-col gap-6">
      <div>
        <Button variant="ghost" size="sm" render={<Link to="/apps" />}>
          Applications
        </Button>
        <h2 className="mt-2 text-lg font-semibold tracking-tight">{app.name}</h2>
        <p className="text-sm text-muted-foreground">
          {app.kind} · {app.status}
          {app.cwd ? ` · ${app.cwd}` : ""}
          {app.interpreter ? ` · ${app.interpreter}` : ""}
        </p>
        {opsKind(app.kind) && (
          <div className="mt-3">
            <OpsQuickActions
              serverId={app.serverId}
              kind={opsKind(app.kind)!}
              target={app.name}
              appId={app.id}
              logPaths={app.logSources.flatMap((source) => (source.path ? [source.path] : []))}
            />
          </div>
        )}
      </div>

      <div role="tablist" aria-label="Application sections" className="flex gap-2">
        {TABS.map((name) => (
          <button
            key={name}
            type="button"
            role="tab"
            id={`tab-${name}`}
            aria-selected={tab === name}
            aria-controls={`panel-${name}`}
            onClick={() => setTab(name)}
            className={cn(
              "rounded-md px-3 py-1.5 text-sm",
              tab === name ? "bg-muted font-medium" : "text-muted-foreground hover:bg-muted/60",
            )}
          >
            {name}
          </button>
        ))}
      </div>

      {tab === "Overview" && (
        <div role="tabpanel" id="panel-Overview" aria-labelledby="tab-Overview" className="flex flex-col gap-6">
          <div className="grid gap-6 lg:grid-cols-3">
            <MetricUplot title="CPU" points={series?.cpu ?? []} formatValue={(v) => `${v.toFixed(1)}%`} color="blue" height={160} />
            <MetricUplot title="Memory" points={series?.mem ?? []} formatValue={(v) => `${(v / 1024 / 1024).toFixed(0)} MB`} color="orange" height={160} />
            <MetricUplot title="Restarts" points={series?.restarts ?? []} formatValue={(v) => v.toFixed(0)} color="vermillion" height={160} />
          </div>
          <div className="grid gap-6 lg:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Process</CardTitle>
              </CardHeader>
              <CardContent className="text-sm">
                <dl className="grid grid-cols-[8rem_1fr] gap-y-2">
                  <dt className="text-muted-foreground">PIDs</dt>
                  <dd>{app.pids.join(", ") || "—"}</dd>
                  <dt className="text-muted-foreground">Ports</dt>
                  <dd>{app.ports.join(", ") || "—"}</dd>
                  <dt className="text-muted-foreground">Domains</dt>
                  <dd>{app.domains.join(", ") || "—"}</dd>
                  <dt className="text-muted-foreground">Tags</dt>
                  <dd>{app.tags.join(", ") || "—"}</dd>
                  <dt className="text-muted-foreground">Restarts</dt>
                  <dd>{app.restarts ?? "—"}</dd>
                </dl>
              </CardContent>
            </Card>
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Log sources</CardTitle>
              </CardHeader>
              <CardContent>
                {app.logSources.length === 0 ? (
                  <p className="text-sm text-muted-foreground">No log sources discovered. Edit the app to add paths.</p>
                ) : (
                  <ul className="flex flex-col gap-2 text-sm">
                    {app.logSources.map((src) => (
                      <li key={src.id}>
                        <span className="font-medium">{src.kind}</span>
                        <span className="text-muted-foreground"> {src.path || src.containerId || src.vhost}</span>
                        {src.fixHint && (
                          <div className="mt-1 flex flex-wrap items-center gap-2">
                            <p className="text-xs text-status-warning">{src.fixHint}</p>
                            <HowToFix text={`${src.kind} ${src.path ?? ""} ${src.fixHint}`} />
                          </div>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
              </CardContent>
            </Card>
          </div>
          <OtelSetup serverId={serverId} appId={app.id} />
        </div>
      )}

      {tab === "Logs" && (
        <div role="tabpanel" id="panel-Logs" aria-labelledby="tab-Logs">
          <LogExplorer serverId={serverId} lockedAppId={app.id} title={`${app.name} logs`} />
        </div>
      )}
      {tab === "Requests" && (
        <div role="tabpanel" id="panel-Requests" aria-labelledby="tab-Requests" className="flex flex-col gap-3">
          <p className="text-sm text-muted-foreground">
            Access-log lines for {app.domains.join(", ") || app.name}. Latency needs{" "}
            <span className="font-mono">$request_time</span> or Apache <span className="font-mono">%D</span> in the log format.
          </p>
          <LogExplorer
            serverId={serverId}
            lockedAppId={app.id}
            requireHttp
            title={`${app.name} requests`}
          />
        </div>
      )}
      {tab === "Errors" && (
        <div role="tabpanel" id="panel-Errors" aria-labelledby="tab-Errors" className="flex flex-col gap-6">
          <ErrorIssues serverId={serverId} appId={app.id} />
          <LogExplorer
            serverId={serverId}
            lockedAppId={app.id}
            levelLock={["error", "fatal"]}
            title={`${app.name} errors`}
          />
        </div>
      )}
      {tab === "Traces" && (
        <div role="tabpanel" id="panel-Traces" aria-labelledby="tab-Traces">
          <TraceBrowser serverId={serverId} appId={app.id} />
        </div>
      )}
    </div>
  );
}
