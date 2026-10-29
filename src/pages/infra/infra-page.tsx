import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { useServersContext } from "@/contexts/servers-context";
import { PageHeader } from "@/components/layout/page-header";
import { EmptyState, Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { SimpleSelect } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { MetricUplot } from "@/components/charts/metric-uplot";
import { infraApi, useInfra, type Snapshot } from "@/components/infra/api";
import { ResourcePanel, type Column } from "@/components/infra/resource-panel";
import { TargetForm } from "@/components/infra/target-form";
import { UptimePanel } from "@/components/infra/uptime-panel";

const dbColumns: Column[] = [
  { key: "connections", label: "Connections" },
  { key: "qps", label: "Queries / transactions / ops", unit: "rate" },
  { key: "slowQueries", label: "Slow queries" },
  { key: "cacheHitPercent", label: "Cache hit", unit: "percent" },
  { key: "replicationLagSeconds", label: "Replica lag (s)" },
  { key: "sizes", label: "Database sizes", width: "min-w-56" },
];
const lbColumns: Column[] = [
  { key: "weight", label: "Weight" },
  { key: "errorPercent", label: "Error rate", unit: "percent" },
  { key: "tcp", label: "TCP probe" },
  { key: "rate", label: "Rate", unit: "rate" },
  { key: "recentConnectionErrors", label: "Recent connect errors" },
  { key: "activeRequests", label: "Active requests" },
  { key: "failures", label: "Passive failures" },
];
const certificateColumns: Column[] = [
  { key: "daysRemaining", label: "Time remaining", unit: "days" },
  { key: "expiresAtMs", label: "Expires", unit: "date" },
  { key: "subject", label: "Subject" },
  { key: "verified", label: "PC trust verified" },
];
interface Panel {
  id: string;
  title: string;
  description: string;
  columns: Column[];
  target?: boolean;
}
export const SECTIONS = {
  databases: {
    title: "Databases",
    description:
      "Connection pressure, throughput, cache efficiency and replication across your data stores.",
    panels: [
      {
        id: "mysql",
        title: "MySQL / MariaDB",
        description:
          "Global status, information_schema sizes and replica status. Throughput needs two successful samples.",
        columns: dbColumns,
        target: true,
      },
      {
        id: "postgres",
        title: "PostgreSQL",
        description:
          "pg_stat_database and pg_stat_activity. The throughput metric counts transactions, and slow queries are active statements over 1 second.",
        columns: dbColumns,
        target: true,
      },
      {
        id: "mongo",
        title: "MongoDB",
        description:
          "serverStatus, listDatabases, replica status and existing profiling data. No profiling is enabled or changed.",
        columns: dbColumns,
        target: true,
      },
    ],
  },
  queues: {
    title: "Queues",
    description:
      "Queue pressure and consumer progress, read directly from the server.",
    panels: [
      {
        id: "rabbitmq",
        title: "RabbitMQ",
        description:
          "Management API on localhost, with rabbitmqctl fallback. Rates are unavailable when only CLI gauges can be read.",
        target: true,
        columns: [
          { key: "depth", label: "Depth" },
          { key: "ready", label: "Ready" },
          { key: "unacknowledged", label: "Unacked" },
          { key: "consumers", label: "Consumers" },
          { key: "publishPerSecond", label: "Publish", unit: "rate" },
          { key: "deliverPerSecond", label: "Deliver", unit: "rate" },
        ],
      },
      {
        id: "kafka",
        title: "Kafka",
        description:
          "Topic partitions and consumer-group offsets at localhost:9092. Lag is log-end offset minus committed offset. Rates use offset changes across two samples. Uncommitted offsets stay unavailable.",
        columns: [
          { key: "partitions", label: "Partitions" },
          { key: "replicationFactor", label: "Replicas" },
          { key: "currentOffset", label: "Committed" },
          { key: "logEndOffset", label: "Log end" },
          { key: "lag", label: "Lag" },
          { key: "consumers", label: "Assigned consumer" },
          { key: "consumePerSecond", label: "Consume", unit: "rate" },
          { key: "producePerSecond", label: "Produce", unit: "rate" },
          { key: "consumer", label: "Consumer" },
        ],
      },
    ],
  },
  balancers: {
    title: "Load balancers",
    description:
      "Backend health and traffic signals, with the evidence behind each status.",
    panels: [
      {
        id: "haproxy",
        title: "HAProxy",
        description:
          "Stats socket or CSV page on localhost:8404. Error ratio is cumulative response errors / sessions.",
        columns: lbColumns,
      },
      {
        id: "nginx_upstreams",
        title: "Nginx upstreams",
        description:
          "nginx -T, recent log samples and TCP probes from the server. Explicitly disabled backends remain down.",
        columns: lbColumns,
      },
      {
        id: "traefik",
        title: "Traefik",
        description:
          "Service API on localhost:8080. Backend status comes from Traefik. Weights and error rates appear only when the source reports them.",
        columns: lbColumns,
      },
      {
        id: "caddy",
        title: "Caddy",
        description:
          "Admin API on localhost:2019. Passive failure counts and TCP reachability are separate from HTTP health.",
        columns: lbColumns,
      },
    ],
  },
  containers: {
    title: "Containers",
    description:
      "Runtime health, resource use and orchestration across Docker and Kubernetes.",
    panels: [
      {
        id: "docker_infra",
        title: "Docker",
        description:
          "Container status, a one-shot stats sample and inspect metadata. CPU can exceed 100% when a container uses multiple cores.",
        columns: [
          { key: "cpuPercent", label: "CPU", unit: "percent" },
          { key: "memory", label: "Memory / limit" },
          { key: "memoryPercent", label: "Memory", unit: "percent" },
          { key: "restarts", label: "Restarts" },
          { key: "health", label: "Health" },
          { key: "ports", label: "Published ports" },
          { key: "composeProject", label: "Compose project" },
        ],
      },
      {
        id: "kubernetes",
        title: "Kubernetes / k3s",
        description:
          "Nodes and pods from the existing kubeconfig. Pending pods and restart counts remain visible alongside node readiness.",
        columns: [
          { key: "kind", label: "Kind" },
          { key: "node", label: "Node" },
          { key: "restarts", label: "Restarts" },
          { key: "readyContainers", label: "Ready containers" },
          { key: "reason", label: "Reason" },
        ],
      },
    ],
  },
  certificates: {
    title: "Certificates and uptime",
    description:
      "Compare server certificate files with public TLS, DNS and HTTP checks from this PC.",
    panels: [
      {
        id: "certificates",
        title: "Server certificate files",
        description:
          "Public certificate files under /etc/letsencrypt/live and configured nginx/Apache certificate paths. Warning below 21 days. Critical below 7 days.",
        columns: certificateColumns,
      },
      {
        id: "certificates_pc",
        title: "Public TLS certificates",
        description:
          "Every concrete discovered domain is queued for a TLS/SNI inspection from this PC on port 443. Wildcard and regex names require a concrete hostname.",
        columns: certificateColumns,
      },
    ],
  },
  cron: {
    title: "Cron",
    description:
      "Scheduled work across user crontabs, /etc/cron.d and systemd timers.",
    panels: [
      {
        id: "cron",
        title: "Jobs and timers",
        description:
          "Cron next runs use the server timezone, including CRON_TZ. Timer results include the last service exit code. Cron logs often record dispatch without an exit result.",
        columns: [
          { key: "user", label: "User" },
          { key: "source", label: "Source" },
          { key: "schedule", label: "Schedule" },
          { key: "timezone", label: "Timezone" },
          { key: "nextRunMs", label: "Next cron run", unit: "date" },
          { key: "nextRun", label: "Next timer / status" },
          { key: "lastRun", label: "Last timer run" },
          { key: "lastResult", label: "Last result", width: "min-w-72" },
          { key: "exitCode", label: "Exit code" },
        ],
      },
    ],
  },
  capacity: {
    title: "Capacity",
    description:
      "Disk growth forecasts from up to 30 days of locally stored history, one model per mount.",
    panels: [
      {
        id: "capacity",
        title: "Disk growth forecast",
        description:
          "Hourly samples feed a linear regression. Forecasts require 6 samples spanning at least 24 hours. Confidence considers fit and observation length.",
        columns: [
          { key: "usedPercent", label: "Used", unit: "percent" },
          { key: "usedBytes", label: "Used", unit: "bytes" },
          { key: "totalBytes", label: "Capacity", unit: "bytes" },
          { key: "growthBytesPerDay", label: "Growth / day", unit: "bytes" },
          { key: "daysUntilFull", label: "Full in", unit: "days" },
          { key: "rSquared", label: "R²" },
          { key: "sampleCount", label: "Hourly samples" },
        ],
      },
    ],
  },
  security: {
    title: "Security",
    description:
      "Authentication activity, listening sockets and patch posture. All observations are read-only.",
    panels: [
      {
        id: "ssh_security",
        title: "Failed SSH authentication",
        description:
          "Top source IPs and hourly event counts from the journal, auth.log or secure. Counts describe log events, which can exceed individual attempts.",
        columns: [{ key: "attempts", label: "Failed events" }],
      },
      {
        id: "ports",
        title: "Listening ports",
        description:
          "Wildcard binds on 0.0.0.0 or IPv6 are flagged. Actual internet exposure also depends on firewall and network rules.",
        columns: [
          { key: "address", label: "Local address" },
          { key: "process", label: "Owning process", width: "min-w-64" },
        ],
      },
      {
        id: "updates",
        title: "Pending package updates",
        description:
          "Existing apt/dnf cache only. Security classification follows repository metadata. Stale caches may omit available updates.",
        columns: [
          { key: "version", label: "Available version" },
          { key: "source", label: "Repository / arch" },
        ],
      },
      {
        id: "fail2ban",
        title: "Fail2ban",
        description:
          "Jail status and current banned IPs from fail2ban-client. No bans or configuration changes are made.",
        columns: [
          { key: "banned", label: "Currently banned" },
          { key: "failed", label: "Total failed" },
          { key: "bannedIps", label: "Banned IPs", width: "min-w-64" },
        ],
      },
      {
        id: "last_logins",
        title: "Recent logins",
        description:
          "The latest recorded sessions from wtmp via last. This records sessions, rather than proving that an account is safe.",
        columns: [
          { key: "session", label: "Session details", width: "min-w-96" },
        ],
      },
    ],
  },
} satisfies Record<
  string,
  { title: string; description: string; panels: Panel[] }
>;
export type Section = keyof typeof SECTIONS;

export function InfraPage({ section }: { section: Section }) {
  const { servers, selectedServerId, statusById } = useServersContext();
  const server = servers.find((s) => s.id === selectedServerId);
  const config = SECTIONS[section];
  const { data, error, reload } = useInfra(
    selectedServerId,
    config.panels.map((p) => p.id).join(","),
    section === "capacity",
  );
  const [busy, setBusy] = useState(false);
  const [showMissing, setShowMissing] = useState(false);
  const snapshots = new Map(data?.map((s) => [s.collector, s]));
  const connected =
    selectedServerId &&
    ["connected", "degraded"].includes(
      statusById.get(selectedServerId)?.status ?? "",
    );
  if (!server || !selectedServerId)
    return (
      <EmptyState
        title="No server selected"
        description="Select a server in the sidebar to explore its infrastructure."
        action={
          <Button render={<Link to="/settings" />}>Server settings</Button>
        }
      />
    );
  const panels: Panel[] = config.panels;
  const relevant = panels
    .map((p) => snapshots.get(p.id))
    .filter((s): s is Snapshot => !!s);
  const detected = relevant.filter((s) => s.phase !== "not_installed");
  const resources = detected.reduce((n, s) => n + s.rows.length, 0);
  const attention = detected.reduce(
    (n, s) =>
      n +
      s.rows.filter((r) =>
        [
          "critical",
          "warning",
          "down",
          "not_ready",
          "Pending",
          "unhealthy",
          "security",
        ].includes(r.state),
      ).length,
    0,
  );
  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow={`Infrastructure / ${server.name}`}
        title={config.title}
        description={config.description}
        actions={
        <Button
          size="sm"
          variant="outline"
          disabled={busy || !connected}
          onClick={() => {
            setBusy(true);
            void infraApi
              .refresh(selectedServerId)
              .then(() => {
                reload();
                toast.success("Collector refresh requested");
              })
              .catch((e) => toast.error(String(e)))
              .finally(() => setBusy(false));
          }}
        >
          <RefreshCw aria-hidden className="size-4" />
          {busy ? "Requesting…" : "Refresh collectors"}
        </Button>
        }
      />
      {!connected && (
        <Notice tone="warning" title="Server disconnected">
          Remote results below may be stale.{" "}
          <Link to="/" className="underline">
            Connect from Overview
          </Link>{" "}
          to resume collection. PC uptime checks continue while the app is open.
        </Notice>
      )}
      {error && (
        <Notice tone="critical" title="Could not load infrastructure">
          {error} Check the connection and retry.{" "}
          <Button size="sm" variant="outline" onClick={reload}>
            Retry
          </Button>
        </Notice>
      )}
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        {[
          ["Resources", resources],
          ["Need attention", attention],
          [
            "Collectors reporting",
            detected.filter((s) => ["ok", "partial"].includes(s.phase)).length,
          ],
          [
            "Unavailable collectors",
            detected.filter((s) =>
              ["error", "permission_denied", "timed_out", "auth_needed", "installed_stopped"].includes(s.phase),
            ).length,
          ],
        ].map(([label, value]) => (
          <div
            key={String(label)}
            className="rounded-xl border border-border bg-card p-4"
          >
            <p className="text-xs text-muted-foreground">{label}</p>
            <p className="mt-2 font-mono text-2xl tabular-nums">
              {data ? value : "—"}
            </p>
          </div>
        ))}
      </div>
      {!data && !error && (
        <div className="grid gap-6 lg:grid-cols-2">
          <Skeleton className="h-64 w-full" />
          <Skeleton className="h-64 w-full" />
        </div>
      )}
      {section === "certificates" && (
        <UptimePanel key={selectedServerId} serverId={selectedServerId} />
      )}
      {section === "capacity" && (
        <CapacityChart snapshot={snapshots.get("capacity")} />
      )}
      {section === "security" && (
        <SecurityChart snapshot={snapshots.get("ssh_security")} />
      )}
      {data && panels.some((panel) => snapshots.get(panel.id)?.phase === "not_installed") && (
        <p className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
          <span>
            Not detected on this server:{" "}
            {panels
              .filter((panel) => snapshots.get(panel.id)?.phase === "not_installed")
              .map((panel) => panel.title)
              .join(", ")}
            .
          </span>
          <Button type="button" size="sm" variant="ghost" onClick={() => setShowMissing((value) => !value)}>
            {showMissing ? "Hide" : "Show anyway"}
          </Button>
        </p>
      )}
      {data &&
        panels
          .filter((panel) => showMissing || snapshots.get(panel.id)?.phase !== "not_installed")
          .map((panel) => (
          <ResourcePanel
            key={`${selectedServerId}-${panel.id}`}
            title={panel.title}
            description={panel.description}
            snapshot={snapshots.get(panel.id)}
            columns={panel.columns}
          >
            {panel.target && (
              <TargetForm
                serverId={selectedServerId}
                engine={panel.id}
                onSaved={reload}
              />
            )}
          </ResourcePanel>
        ))}
      <p className="text-xs text-muted-foreground">
        Read-only observations over the existing SSH session. Each collector has
        its own timeout. “Unavailable” means the source did not report a value;
        throughput needs two successful samples.
      </p>
    </div>
  );
}
function CapacityChart({ snapshot }: { snapshot?: Snapshot }) {
  const [mount, setMount] = useState("");
  const row = snapshot?.rows.find((r) => r.id === mount) ?? snapshot?.rows[0];
  const points = useMemo(
    () =>
      Array.isArray(row?.fields.history)
        ? (row.fields.history as { t: number; v: number }[])
        : [],
    [row],
  );
  if (!row) return null;
  return (
    <section className="space-y-3">
      <label className="flex items-center gap-3 text-sm">
        Mount history
        <SimpleSelect
          value={row.id}
          onValueChange={setMount}
          options={(snapshot?.rows ?? []).map((r) => ({ value: r.id, label: r.name }))}
        />
      </label>
      {points.length > 1 ? (
        <MetricUplot
          title={`Disk used · ${row.name}`}
          points={points}
          color="purple"
          formatValue={(v) => `${v.toFixed(1)}%`}
        />
      ) : (
        <Notice title="Building capacity history">
          Collection runs every 5 minutes. A forecast appears after enough
          hourly samples are stored.
        </Notice>
      )}
    </section>
  );
}
function SecurityChart({ snapshot }: { snapshot?: Snapshot }) {
  const points = useMemo(() => {
    const timeline = snapshot?.rows[0]?.fields.timeline;
    return Array.isArray(timeline)
      ? timeline
          .map((item) => ({
            t:
              typeof item.tsMs === "number"
                ? item.tsMs
                : Date.parse(`${item.hour}:00:00Z`),
            v: Number(item.count),
          }))
          .filter((p) => Number.isFinite(p.t))
      : [];
  }, [snapshot]);
  if (points.length < 2) return null;
  return (
    <MetricUplot
      title="Failed SSH log events per hour"
      points={points}
      color="vermillion"
      fill={false}
      formatValue={(v) => v.toFixed(0)}
    />
  );
}
