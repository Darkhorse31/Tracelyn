import { Link } from "react-router-dom";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { useInfra } from "./api";
import { Status } from "./resource-panel";

const GROUPS = [
  { title: "Databases", to: "databases", ids: ["mysql", "postgres", "mongo"] },
  { title: "Queues", to: "queues", ids: ["rabbitmq", "kafka"] },
  {
    title: "Load balancers",
    to: "balancers",
    ids: ["haproxy", "nginx_upstreams", "traefik", "caddy"],
  },
  {
    title: "Containers",
    to: "containers",
    ids: ["docker_infra", "kubernetes"],
  },
  {
    title: "Certificates",
    to: "certificates",
    ids: ["certificates", "certificates_pc"],
  },
  { title: "Cron", to: "cron", ids: ["cron"] },
  { title: "Capacity", to: "capacity", ids: ["capacity"] },
  {
    title: "Security",
    to: "security",
    ids: ["ssh_security", "ports", "updates", "fail2ban", "last_logins"],
  },
];
export function InfraSummaryCard({ serverId }: { serverId: string }) {
  const { data, error } = useInfra(serverId, "", false, true);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Infrastructure</CardTitle>
        <CardDescription>
          {error
            ? `Could not load infrastructure: ${error}. Check the connection and local database access.`
            : "Service health, certificate risk and capacity across this server. Open a category for source details and fix hints."}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          {GROUPS.map((group) => {
            const snapshots =
              data?.filter((s) => group.ids.includes(s.collector)) ?? [];
            const resources = snapshots.reduce((n, s) => n + s.rows.length, 0);
            const issues = snapshots.reduce(
              (n, s) =>
                n +
                s.rows.filter((r) =>
                  [
                    "critical",
                    "warning",
                    "down",
                    "Pending",
                    "not_ready",
                    "unhealthy",
                    "security",
                  ].includes(r.state),
                ).length,
              0,
            );
            const phase = !data
              ? "loading"
              : issues
                ? "warning"
                : snapshots.some((s) => s.phase === "ok")
                  ? "ok"
                  : snapshots.some((s) => s.phase === "partial")
                    ? "partial"
                    : "unknown";
            return (
              <Link
                key={group.to}
                to={`/infra/${group.to}`}
                className="flex flex-col gap-3 rounded-lg border border-border p-4 hover:bg-muted/40"
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="text-sm font-medium">{group.title}</span>
                  <Status value={phase} />
                </div>
                <p className="text-xs text-muted-foreground">
                  {data
                    ? `${resources} resources · ${issues} need attention`
                    : "Waiting for samples"}
                </p>
              </Link>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}
