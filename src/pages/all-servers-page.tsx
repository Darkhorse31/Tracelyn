import { useMemo, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { ArrowDown, ArrowUp, Download, Loader2, Plus, Upload } from "lucide-react";
import { toast } from "sonner";
import { PageHeader } from "@/components/layout/page-header";
import { FleetToolbar } from "@/components/servers/fleet-toolbar";
import { ServerFormDialog } from "@/components/servers/server-form-dialog";
import { ServerImportDialog } from "@/components/servers/server-import-dialog";
import { ConnectionStatusBadge } from "@/components/status/connection-status-badge";
import { EmptyState, Notice } from "@/components/status/notice";
import { PageSkeleton } from "@/components/status/page-skeleton";
import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { useServersContext } from "@/contexts/servers-context";
import { Masked } from "@/contexts/privacy-context";
import {
  readPreference,
  useRememberedState,
} from "@/hooks/use-remembered-state";
import { useOverviewMetrics } from "@/hooks/use-overview-metrics";
import { useFleetSummary } from "@/hooks/use-fleet-summary";
import { exportServers } from "@/lib/fleet-api";
import {
  FLEET_SORT_COLUMNS,
  filterAndSortFleet,
  groupFleetRows,
  rowMatchesFilters,
  seenLabel,
  statusCounts,
  tagsFromStored,
  toggleFleetSort,
  type FleetSortKey,
  type FleetViewRow,
} from "@/lib/fleet-view";
import type { ConnectionStatus, ServerRecord } from "@/lib/types";
import { cn } from "cn";

const STRIPE: Record<ConnectionStatus, string> = {
  disconnected: "bg-muted-foreground",
  connecting: "bg-status-info",
  connected: "bg-status-ok",
  degraded: "bg-status-warning",
  offline: "bg-status-critical",
};

function Percent({ value }: { value?: number | null }) {
  if (value == null || !Number.isFinite(value))
    return (
      <span className="text-muted-foreground" aria-label="No sample">
        —
      </span>
    );
  const tone =
    value >= 90 ? "critical" : value >= 75 ? "warning" : "steady";
  return (
    <span className="flex min-w-16 flex-col gap-1 tabular-nums">
      <span
        className={cn(
          tone === "critical" && "font-medium text-status-critical",
          tone === "warning" && "font-medium text-status-warning",
        )}
      >
        {value.toFixed(1)}%
      </span>
      <span className="h-1 w-full rounded-full bg-muted" aria-hidden>
        <span
          className={cn(
            "block h-1 rounded-full transition-[width] duration-150",
            tone === "critical" && "bg-status-critical",
            tone === "warning" && "bg-status-warning",
            tone === "steady" && "bg-primary",
          )}
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
  const [tags, setTags] = useRememberedState<string[]>(
    "fleet:labels",
    tagsFromStored(readPreference("fleet:label", "all")),
  );
  const [sortKey, setSortKey] = useRememberedState<FleetSortKey>(
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
  const tagOptions = [...new Set([...labels, ...tags])].sort();
  const fleetRows = servers.map((server) => {
    const stored = byId.get(server.id),
      live = metrics.get(server.id),
      status = statusById.get(server.id)?.status ?? "disconnected";
    const fresh = status === "connected" && live !== undefined;
    const row: FleetViewRow & { server: ServerRecord } = {
      server,
      id: server.id,
      name: server.name,
      host: server.host,
      username: server.username,
      labels: server.labels,
      status,
      cpu: fresh ? live.cpuPercent : stored?.cpuPercent,
      memory: fresh ? live.memoryPercent : stored?.memoryPercent,
      disk: fresh ? live.diskPercent : stored?.diskPercent,
      apps: stored?.appCount,
      alerts: stored?.activeAlertCount,
      lastSeen: stored?.lastSeenMs,
    };
    return row;
  });
  const filters = { query, status: statusFilter, tags };
  const rows = filterAndSortFleet(fleetRows, filters, sortKey, ascending);
  const counts = statusCounts(
    fleetRows.filter((row) =>
      rowMatchesFilters(row, { query, status: "all", tags }),
    ),
  );
  const groups = groupFleetRows(rows, group);
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
  const sort = (key: FleetSortKey) => {
    const next = toggleFleetSort(sortKey, ascending, key);
    setSortKey(next.sortKey);
    setAscending(next.ascending);
  };
  const clearFilters = () => {
    setQuery("");
    setStatusFilter("all");
    setTags([]);
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
    <div className="flex flex-col gap-6">
      <PageHeader
        title="All servers"
        description="Current health across your fleet. Latest samples are shown here, with last seen for disconnected servers."
        actions={
          <>
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
          </>
        }
      />
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
          <div className="grid items-stretch gap-4 sm:grid-cols-3">
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
          <FleetToolbar
            query={query}
            onQueryChange={setQuery}
            status={statusFilter}
            onStatusChange={setStatusFilter}
            counts={counts}
            tags={tags}
            onTagsChange={setTags}
            tagOptions={tagOptions}
            sortKey={sortKey}
            ascending={ascending}
            onSortKeyChange={setSortKey}
            onAscendingChange={setAscending}
            view={view}
            onViewChange={setView}
            group={group}
            onGroupChange={setGroup}
            onClear={clearFilters}
          />
          <p role="status" className="text-xs text-muted-foreground">
            {rows.length} of {servers.length} servers · Filters and layout are
            remembered
          </p>
          {!rows.length && (
            <EmptyState
              title="No matching servers"
              description="Try another name, status, or tag."
              action={
                <Button variant="outline" onClick={clearFilters}>
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
                <div className="flex items-center gap-2">
                  <h3 className="text-sm font-medium">{section.label}</h3>
                  <Badge variant="secondary">{section.rows.length}</Badge>
                </div>
              )}
              {view === "table" ? (
                section.rows.length > 0 && (
                  <div
                    role="region"
                    aria-label={`${section.label || "All servers"} table`}
                    tabIndex={0}
                    className="overflow-auto rounded-xl border border-border focus-visible:ring-3 focus-visible:ring-ring"
                  >
                    <table className="w-full min-w-[1120px] text-left text-sm">
                      <caption className="sr-only">
                        Server health and quick actions
                      </caption>
                      <thead className="sticky top-0 z-10 bg-muted">
                        <tr>
                          {FLEET_SORT_COLUMNS.map((c) => (
                            <th
                              scope="col"
                              key={c.key}
                              className="p-2"
                              aria-sort={
                                sortKey === c.key
                                  ? ascending
                                    ? "ascending"
                                    : "descending"
                                  : "none"
                              }
                            >
                              <button
                                type="button"
                                className="flex h-8 items-center gap-1 rounded-md px-1 font-medium transition-colors duration-150 hover:bg-background/70 focus-visible:ring-3 focus-visible:ring-ring"
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
                          <th scope="col" className="p-2 font-medium">
                            Quick actions
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {section.rows.map((row) => (
                          <tr
                            key={row.server.id}
                            className="border-t border-border transition-colors duration-150 hover:bg-muted/40 focus-within:bg-muted/30"
                          >
                            <th
                              scope="row"
                              className="max-w-64 p-3 font-normal"
                            >
                              <button
                                type="button"
                                className="max-w-full truncate rounded-md text-left font-medium tracking-tight hover:underline focus-visible:ring-3 focus-visible:ring-ring"
                                onClick={() =>
                                  openPage(row.server.id, "/server")
                                }
                              >
                                {row.server.name}
                              </button>
                              <p className="truncate font-mono text-xs text-muted-foreground">
                                <Masked text={row.server.host} />
                              </p>
                              {row.server.labels.length > 0 && (
                                <div className="mt-1.5 flex flex-wrap gap-1">
                                  {row.server.labels.map((label) => (
                                    <Badge
                                      key={label}
                                      variant="outline"
                                      className={cn(
                                        "cursor-pointer transition-colors duration-150 hover:bg-muted",
                                        tags.includes(label) && "bg-muted",
                                      )}
                                      render={
                                        <button
                                          type="button"
                                          aria-pressed={tags.includes(label)}
                                          aria-label={`Filter tag ${label}`}
                                          onClick={() => setTags([label])}
                                        />
                                      }
                                    >
                                      {label}
                                    </Badge>
                                  ))}
                                </div>
                              )}
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
                    <Card
                      key={row.server.id}
                      className="relative transition-shadow duration-150 hover:shadow-md focus-within:ring-3 focus-within:ring-ring"
                    >
                      <span
                        aria-hidden
                        className={cn(
                          "absolute inset-y-3 left-0 w-0.5 rounded-full",
                          STRIPE[row.status],
                        )}
                      />
                      <CardHeader className="flex flex-col gap-2">
                        <div className="flex items-start justify-between gap-3">
                          <CardTitle>
                            <button
                              type="button"
                              className="rounded-md text-left tracking-tight transition-colors duration-150 hover:underline focus-visible:ring-3 focus-visible:ring-ring"
                              onClick={() => openPage(row.server.id, "/server")}
                            >
                              {row.server.name}
                            </button>
                          </CardTitle>
                          <ConnectionStatusBadge status={row.status} />
                        </div>
                        <CardDescription className="truncate font-mono text-xs">
                          <Masked
                            text={`${row.server.username}@${row.server.host}:${row.server.port}`}
                          />
                        </CardDescription>
                        {row.server.labels.length > 0 && (
                          <div className="flex flex-wrap gap-1">
                            {row.server.labels.map((label) => (
                              <Badge
                                key={label}
                                variant="outline"
                                className={cn(
                                  "cursor-pointer transition-colors duration-150 hover:bg-muted",
                                  tags.includes(label) && "bg-muted",
                                )}
                                render={
                                  <button
                                    type="button"
                                    aria-pressed={tags.includes(label)}
                                    aria-label={`Filter tag ${label}`}
                                    onClick={() => setTags([label])}
                                  />
                                }
                              >
                                {label}
                              </Badge>
                            ))}
                          </div>
                        )}
                      </CardHeader>
                      <CardContent className="flex flex-col gap-3">
                        <dl className="grid grid-cols-3 gap-3">
                          {[
                            ["CPU", row.cpu],
                            ["Memory", row.memory],
                            ["Disk", row.disk],
                          ].map(([label, value]) => (
                            <div key={String(label)} className="min-w-0">
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
                        <div className="flex flex-wrap items-center gap-2 text-xs">
                          <Button
                            size="sm"
                            variant="link"
                            aria-label={`Applications for ${row.server.name}`}
                            onClick={() => openPage(row.server.id, "/apps")}
                          >
                            {row.apps ?? "—"} apps
                          </Button>
                          <Button
                            size="sm"
                            variant="link"
                            aria-label={`Alerts for ${row.server.name}`}
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
                      </CardContent>
                      <CardFooter className="flex-wrap gap-2">
                        {actions(row.server, row.status)}
                      </CardFooter>
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
