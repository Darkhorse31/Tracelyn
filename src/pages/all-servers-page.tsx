import { useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import {
  ArrowDown,
  ArrowUp,
  Download,
  LayoutGrid,
  List,
  Loader2,
  Plus,
  Upload,
} from "lucide-react";
import { toast } from "sonner";
import { ServerFormDialog } from "@/components/servers/server-form-dialog";
import { ServerImportDialog } from "@/components/servers/server-import-dialog";
import {
  CONNECTION_STATUS_LABELS,
  ConnectionStatusBadge,
} from "@/components/status/connection-status-badge";
import { EmptyState, Notice } from "@/components/status/notice";
import { PageSkeleton } from "@/components/status/page-skeleton";
import { Button, buttonVariants } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { SimpleSelect } from "@/components/ui/select";
import { useServersContext } from "@/contexts/servers-context";
import { Masked } from "@/contexts/privacy-context";
import { useRememberedState } from "@/hooks/use-remembered-state";
import { useOverviewMetrics } from "@/hooks/use-overview-metrics";
import { useFleetSummary } from "@/hooks/use-fleet-summary";
import { exportServers } from "@/lib/fleet-api";
import type { ServerRecord } from "@/lib/types";

type SortKey =
  | "name"
  | "status"
  | "cpu"
  | "memory"
  | "disk"
  | "apps"
  | "alerts"
  | "lastSeen";
const COLUMNS: { key: SortKey; label: string }[] = [
  { key: "name", label: "Server" },
  { key: "status", label: "Status" },
  { key: "cpu", label: "CPU" },
  { key: "memory", label: "Memory" },
  { key: "disk", label: "Disk" },
  { key: "apps", label: "Apps" },
  { key: "alerts", label: "Active alerts" },
  { key: "lastSeen", label: "Last seen" },
];
const STATUS_RANK: Record<string, number> = {
  offline: 0,
  degraded: 1,
  disconnected: 2,
  connecting: 3,
  connected: 4,
};
function seenLabel(timestamp: number | null | undefined) {
  if (!timestamp) return "Never";
  const age = Math.max(0, Date.now() - timestamp);
  if (age < 15_000) return "Just now";
  if (age < 60_000) return `${Math.floor(age / 1000)}s ago`;
  if (age < 3_600_000) return `${Math.floor(age / 60_000)}m ago`;
  if (age < 86_400_000) return `${Math.floor(age / 3_600_000)}h ago`;
  return new Date(timestamp).toLocaleDateString();
}
function Percent({ value }: { value?: number | null }) {
  if (value == null || !Number.isFinite(value))
    return (
      <span className="text-muted-foreground" aria-label="No sample">
        —
      </span>
    );
  return (
    <span className="flex min-w-16 flex-col gap-1 tabular-nums">
      <span>{value.toFixed(1)}%</span>
      <span className="h-1 w-full rounded-full bg-muted" aria-hidden>
        <span
          className="block h-1 rounded-full bg-primary"
          style={{ width: `${Math.min(100, Math.max(0, value))}%` }}
        />
      </span>
    </span>
  );
}

export function AllServersPage() {
  const {
    servers,
    statusById,
    loading,
    error: serverError,
    refresh: refreshServers,
    setSelectedServerId,
    connect,
    disconnect,
  } = useServersContext();
  const metrics = useOverviewMetrics(true);
  const summary = useFleetSummary();
  const navigate = useNavigate();
  const [query, setQuery] = useRememberedState("fleet:query", "");
  const [statusFilter, setStatusFilter] = useRememberedState(
    "fleet:status",
    "all",
  );
  const [labelFilter, setLabelFilter] = useRememberedState(
    "fleet:label",
    "all",
  );
  const [sortKey, setSortKey] = useRememberedState<SortKey>(
    "fleet:sort",
    "status",
  );
  const [ascending, setAscending] = useRememberedState("fleet:ascending", true);
  const [view, setView] = useRememberedState("fleet:view", "grid");
  const [group, setGroup] = useRememberedState("fleet:group", false);
  const [editing, setEditing] = useState<ServerRecord | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [busy, setBusy] = useState<Set<string>>(new Set());
  const byId = useMemo(
    () => new Map(summary.rows.map((row) => [row.serverId, row])),
    [summary.rows],
  );
  const labels = [
    ...new Set(servers.flatMap((server) => server.labels)),
  ].sort();
  const rows = servers
    .map((server) => {
      const stored = byId.get(server.id),
        live = metrics.get(server.id),
        status = statusById.get(server.id)?.status ?? "disconnected";
      const fresh = status === "connected" && live !== undefined;
      return {
        server,
        status,
        cpu: fresh ? live.cpuPercent : stored?.cpuPercent,
        memory: fresh ? live.memoryPercent : stored?.memoryPercent,
        disk: fresh ? live.diskPercent : stored?.diskPercent,
        apps: stored?.appCount,
        alerts: stored?.activeAlertCount,
        lastSeen: stored?.lastSeenMs,
      };
    })
    .filter(
      (row) =>
        (statusFilter === "all" || row.status === statusFilter) &&
        (labelFilter === "all" || row.server.labels.includes(labelFilter)) &&
        `${row.server.name} ${row.server.host} ${row.server.username} ${row.server.labels.join(" ")}`
          .toLowerCase()
          .includes(query.trim().toLowerCase()),
    )
    .sort((a, b) => {
      if (sortKey === "name")
        return (
          (ascending ? 1 : -1) * a.server.name.localeCompare(b.server.name)
        );
      const first = sortKey === "status" ? STATUS_RANK[a.status] : a[sortKey];
      const second = sortKey === "status" ? STATUS_RANK[b.status] : b[sortKey];
      if (first == null || second == null)
        return first == null && second == null
          ? a.server.name.localeCompare(b.server.name)
          : first == null
            ? 1
            : -1;
      return (
        (ascending ? 1 : -1) * (first - second) ||
        a.server.name.localeCompare(b.server.name)
      );
    });
  const groups = group
    ? [...new Set(rows.map((row) => row.server.labels[0] ?? "Untagged"))]
        .sort()
        .map((label) => ({
          label,
          rows: rows.filter(
            (row) => (row.server.labels[0] ?? "Untagged") === label,
          ),
        }))
    : [{ label: "", rows }];
  const openPage = (id: string, page: string) => {
    setSelectedServerId(id);
    navigate(page);
  };
  const toggleConnection = async (id: string, connected: boolean) => {
    setBusy((previous) => new Set(previous).add(id));
    try {
      if (connected) await disconnect(id);
      else await connect(id);
    } catch (err) {
      toast.error(connected ? "Could not disconnect" : "Could not connect", {
        description: String(err),
      });
    } finally {
      setBusy((previous) => {
        const next = new Set(previous);
        next.delete(id);
        return next;
      });
      summary.refresh();
    }
  };
  const sort = (key: SortKey) => {
    if (sortKey === key) setAscending((previous) => !previous);
    else {
      setSortKey(key);
      setAscending(key !== "alerts");
    }
  };
  const actions = (server: ServerRecord, status: string) => (
    <div
      className="flex flex-wrap items-center gap-1"
      aria-label={`Actions for ${server.name}`}
    >
      <Button
        size="sm"
        variant="outline"
        aria-label={`Open ${server.name}`}
        onClick={() => openPage(server.id, "/server")}
      >
        Open
      </Button>
      <Button
        size="sm"
        variant="ghost"
        disabled={busy.has(server.id) || status === "connecting"}
        aria-label={`${status === "connected" || status === "degraded" ? "Disconnect" : "Connect"} ${server.name}`}
        onClick={() =>
          void toggleConnection(
            server.id,
            status === "connected" || status === "degraded",
          )
        }
      >
        {busy.has(server.id) && (
          <Loader2 className="animate-spin" aria-hidden />
        )}
        {status === "connected" || status === "degraded"
          ? "Disconnect"
          : status === "connecting"
            ? "Connecting…"
            : "Connect"}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        aria-label={`Edit ${server.name} and tags`}
        onClick={() => {
          setEditing(server);
          setFormOpen(true);
        }}
      >
        Edit
      </Button>
      <Button
        size="sm"
        variant="ghost"
        aria-label={`Setup Guide for ${server.name}`}
        onClick={() => openPage(server.id, "/setup")}
      >
        Setup
      </Button>
    </div>
  );

  if (loading) return <PageSkeleton />;
  if (serverError)
    return (
      <Notice tone="critical" title="Could not load servers">
        <Masked text={serverError} />
        <Button
          className="mt-3"
          variant="outline"
          onClick={() =>
            void refreshServers().catch((err) => toast.error(String(err)))
          }
        >
          Retry
        </Button>
      </Notice>
    );
  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h2 className="text-xl font-semibold tracking-tight">All servers</h2>
          <p className="mt-1 text-sm text-muted-foreground">
            Current health across your fleet. Latest samples are shown here,
            with last seen for disconnected servers.
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={() => setImportOpen(true)}>
            <Upload aria-hidden />
            Import
          </Button>
          <Button
            variant="outline"
            disabled={!servers.length || exporting}
            onClick={() => {
              setExporting(true);
              void exportServers()
                .then((saved) => {
                  if (saved)
                    toast.success("Server profiles exported without secrets");
                })
                .catch((err) =>
                  toast.error("Could not export profiles", {
                    description: String(err),
                  }),
                )
                .finally(() => setExporting(false));
            }}
          >
            <Download aria-hidden />
            Export
          </Button>
          <Button
            onClick={() => {
              setEditing(null);
              setFormOpen(true);
            }}
          >
            <Plus aria-hidden />
            Add server
          </Button>
        </div>
      </div>
      {servers.length === 0 ? (
        <EmptyState
          title="Bring your first server into view"
          description="Add a Linux VPS or import your SSH profiles. The setup wizard can guide you through connection checks."
          action={
            <Link className={buttonVariants()} to="/welcome">Open setup wizard</Link>
          }
        />
      ) : (
        <>
          <div className="grid gap-3 sm:grid-cols-3">
            <Card>
              <CardContent className="p-4">
                <p className="text-xs text-muted-foreground">Servers</p>
                <p className="mt-1 text-2xl font-semibold tabular-nums">
                  {servers.length}
                </p>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="p-4">
                <p className="text-xs text-muted-foreground">Connected</p>
                <p className="mt-1 text-2xl font-semibold tabular-nums">
                  {
                    servers.filter(
                      (s) => statusById.get(s.id)?.status === "connected",
                    ).length
                  }
                </p>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="p-4">
                <p className="text-xs text-muted-foreground">Active alerts</p>
                <p className="mt-1 text-2xl font-semibold tabular-nums">
                  {summary.loading || summary.error
                    ? "—"
                    : summary.rows.reduce(
                        (total, row) => total + row.activeAlertCount,
                        0,
                      )}
                </p>
              </CardContent>
            </Card>
          </div>
          {summary.error && (
            <Notice title="Some summary data is unavailable">
              <Masked text={summary.error} />
              <Button
                variant="outline"
                size="sm"
                className="mt-2"
                onClick={summary.refresh}
              >
                Retry summary
              </Button>
            </Notice>
          )}
          <div className="flex flex-wrap items-end gap-3 rounded-xl border border-border bg-card p-4">
            <div className="min-w-48 flex-1">
              <Label htmlFor="fleet-search">Search servers or tags</Label>
              <Input
                id="fleet-search"
                placeholder="Name, host, or tag"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
            <div>
              <Label htmlFor="fleet-status">Status</Label>
              <SimpleSelect
                id="fleet-status"
                value={statusFilter}
                onValueChange={setStatusFilter}
                options={[
                  { value: "all", label: "All statuses" },
                  ...Object.entries(CONNECTION_STATUS_LABELS).map(
                    ([value, label]) => ({ value, label }),
                  ),
                ]}
              />
            </div>
            <div>
              <Label htmlFor="fleet-tag">Tag</Label>
              <SimpleSelect
                id="fleet-tag"
                value={labelFilter}
                onValueChange={setLabelFilter}
                options={[
                  { value: "all", label: "All tags" },
                  ...labels.map((label) => ({ value: label, label })),
                ]}
              />
            </div>
            <div>
              <Label htmlFor="fleet-sort">Sort by</Label>
              <SimpleSelect
                id="fleet-sort"
                value={sortKey}
                onValueChange={(value) => setSortKey(value as SortKey)}
                options={COLUMNS.map((c) => ({ value: c.key, label: c.label }))}
              />
            </div>
            <Button
              variant="outline"
              size="icon"
              aria-label={ascending ? "Sort descending" : "Sort ascending"}
              onClick={() => setAscending((previous) => !previous)}
            >
              {ascending ? <ArrowUp aria-hidden /> : <ArrowDown aria-hidden />}
            </Button>
            <div className="flex gap-1" role="group" aria-label="Server layout">
              <Button
                size="icon"
                variant={view === "grid" ? "secondary" : "ghost"}
                aria-label="Grid view"
                aria-pressed={view === "grid"}
                onClick={() => setView("grid")}
              >
                <LayoutGrid aria-hidden />
              </Button>
              <Button
                size="icon"
                variant={view === "table" ? "secondary" : "ghost"}
                aria-label="Table view"
                aria-pressed={view === "table"}
                onClick={() => setView("table")}
              >
                <List aria-hidden />
              </Button>
            </div>
            <label className="flex min-h-9 items-center gap-2 text-sm">
              <input
                type="checkbox"
                className="size-4 accent-primary"
                checked={group}
                onChange={(event) => setGroup(event.target.checked)}
              />
              Group by first tag
            </label>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setQuery("");
                setLabelFilter("all");
                setStatusFilter("all");
              }}
            >
              Clear filters
            </Button>
          </div>
          <p role="status" className="text-xs text-muted-foreground">
            {rows.length} of {servers.length} servers · Filters and layout are
            remembered
          </p>
          {!rows.length && (
            <EmptyState
              title="No matching servers"
              description="Try another name, status, or tag."
              action={
                <Button
                  variant="outline"
                  onClick={() => {
                    setQuery("");
                    setStatusFilter("all");
                    setLabelFilter("all");
                  }}
                >
                  Clear filters
                </Button>
              }
            />
          )}
          {groups.map((section) => (
            <section
              key={section.label}
              aria-label={section.label || "Server health"}
              className="flex flex-col gap-3"
            >
              {section.label && (
                <h3 className="font-semibold">
                  {section.label}{" "}
                  <span className="text-sm font-normal text-muted-foreground">
                    {section.rows.length}
                  </span>
                </h3>
              )}
              {view === "table" ? (
                section.rows.length > 0 && (
                  <div
                    role="region"
                    aria-label={`${section.label || "All servers"} table`}
                    tabIndex={0}
                    className="overflow-auto rounded-xl border border-border focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <table className="w-full min-w-[1120px] text-left text-sm">
                      <caption className="sr-only">
                        Server health and quick actions
                      </caption>
                      <thead className="bg-muted/50">
                        <tr>
                          {COLUMNS.map((c) => (
                            <th
                              scope="col"
                              key={c.key}
                              className="p-3"
                              aria-sort={
                                sortKey === c.key
                                  ? ascending
                                    ? "ascending"
                                    : "descending"
                                  : "none"
                              }
                            >
                              <button
                                className="flex items-center gap-1 rounded font-medium focus-visible:ring-2 focus-visible:ring-ring"
                                onClick={() => sort(c.key)}
                              >
                                {c.label}
                                {sortKey === c.key &&
                                  (ascending ? (
                                    <ArrowUp className="size-3" aria-hidden />
                                  ) : (
                                    <ArrowDown className="size-3" aria-hidden />
                                  ))}
                              </button>
                            </th>
                          ))}
                          <th scope="col" className="p-3 font-medium">
                            Quick actions
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {section.rows.map((row) => (
                          <tr
                            key={row.server.id}
                            className="border-t border-border"
                          >
                            <th
                              scope="row"
                              className="max-w-56 p-3 font-normal"
                            >
                              <button
                                className="max-w-full truncate rounded font-semibold hover:underline focus-visible:ring-2 focus-visible:ring-ring"
                                onClick={() =>
                                  openPage(row.server.id, "/server")
                                }
                              >
                                {row.server.name}
                              </button>
                              <p className="truncate text-xs text-muted-foreground">
                                <Masked text={row.server.host} />
                              </p>
                              <p className="mt-1 text-xs text-muted-foreground">
                                {row.server.labels.join(" · ")}
                              </p>
                            </th>
                            <td className="p-3">
                              <ConnectionStatusBadge status={row.status} />
                            </td>
                            <td className="p-3">
                              <Percent value={row.cpu} />
                            </td>
                            <td className="p-3">
                              <Percent value={row.memory} />
                            </td>
                            <td className="p-3">
                              <Percent value={row.disk} />
                            </td>
                            <td className="p-3">
                              <Button
                                variant="link"
                                size="sm"
                                aria-label={`Applications for ${row.server.name}`}
                                onClick={() => openPage(row.server.id, "/apps")}
                              >
                                {row.apps ?? "—"}
                              </Button>
                            </td>
                            <td className="p-3">
                              <Button
                                variant="link"
                                size="sm"
                                aria-label={`Alerts for ${row.server.name}`}
                                onClick={() =>
                                  openPage(row.server.id, "/alerts")
                                }
                              >
                                {row.alerts ?? "—"}
                              </Button>
                            </td>
                            <td
                              className="p-3 whitespace-nowrap"
                              title={
                                row.lastSeen
                                  ? new Date(row.lastSeen).toLocaleString()
                                  : undefined
                              }
                            >
                              {seenLabel(row.lastSeen)}
                            </td>
                            <td className="p-3">
                              {actions(row.server, row.status)}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )
              ) : (
            <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,320px),1fr))] gap-4">
                  {section.rows.map((row) => (
                    <Card key={row.server.id}>
                      <CardHeader>
                        <div className="flex items-start justify-between gap-3">
                          <CardTitle>
                            <button
                              className="rounded text-left hover:underline focus-visible:ring-2 focus-visible:ring-ring"
                              onClick={() => openPage(row.server.id, "/server")}
                            >
                              {row.server.name}
                            </button>
                          </CardTitle>
                          <ConnectionStatusBadge status={row.status} />
                        </div>
                        <p className="truncate font-mono text-xs text-muted-foreground">
                          <Masked
                            text={`${row.server.username}@${row.server.host}:${row.server.port}`}
                          />
                        </p>
                        <div className="flex flex-wrap gap-1">
                          {row.server.labels.map((label) => (
                            <button
                              key={label}
                              onClick={() => setLabelFilter(label)}
                              className="rounded-md border border-border bg-muted px-2 py-1 text-xs hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring"
                              aria-label={`Filter tag ${label}`}
                            >
                              {label}
                            </button>
                          ))}
                        </div>
                      </CardHeader>
                      <CardContent className="flex flex-col gap-4">
                        <dl className="grid grid-cols-3 gap-4">
                          {[
                            ["CPU", row.cpu],
                            ["Memory", row.memory],
                            ["Disk", row.disk],
                          ].map(([label, value]) => (
                            <div key={label}>
                              <dt className="mb-1 text-xs text-muted-foreground">
                                {label}
                              </dt>
                              <dd>
                                <Percent
                                  value={value as number | null | undefined}
                                />
                              </dd>
                            </div>
                          ))}
                        </dl>
                        <div className="flex flex-wrap items-center gap-2 border-t border-border pt-3 text-xs">
                          <Button
                            size="sm"
                            variant="link"
                            onClick={() => openPage(row.server.id, "/apps")}
                          >
                            {row.apps ?? "—"} apps
                          </Button>
                          <Button
                            size="sm"
                            variant="link"
                            onClick={() => openPage(row.server.id, "/alerts")}
                          >
                            {row.alerts ?? "—"} active alerts
                          </Button>
                          <span
                            className="ml-auto text-muted-foreground"
                            title={
                              row.lastSeen
                                ? new Date(row.lastSeen).toLocaleString()
                                : undefined
                            }
                          >
                            Seen {seenLabel(row.lastSeen).toLowerCase()}
                          </span>
                        </div>
                        {statusById.get(row.server.id)?.lastError && (
                          <p className="text-xs text-status-critical">
                            <Masked
                              text={
                                statusById.get(row.server.id)?.lastError ?? ""
                              }
                            />
                          </p>
                        )}
                        {actions(row.server, row.status)}
                      </CardContent>
                    </Card>
                  ))}
                </div>
              )}
            </section>
          ))}
        </>
      )}
      <ServerFormDialog
        open={formOpen}
        onOpenChange={setFormOpen}
        server={editing}
      />
      <ServerImportDialog
        open={importOpen}
        onOpenChange={setImportOpen}
        onImported={summary.refresh}
      />
    </div>
  );
}
