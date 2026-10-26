import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { Boxes, Plus, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { OpsModeBanner } from "@/components/ops/operations-mode-card";
import { OpsQuickActions } from "@/components/ops/quick-actions";
import { EmptyState, Notice } from "@/components/status/notice";
import { DiagnosticsPanel } from "@/components/diagnostics/diagnostics-panel";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { SimpleSelect } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useServersContext } from "@/contexts/servers-context";
import { useApps } from "@/hooks/use-apps";
import { hideApp, mergeApps, rediscoverApps, upsertManualApp } from "@/lib/api";
import {
  APP_KINDS,
  type AppKind,
  type AppStatus,
  type LogSourceKind,
  type TracelynApp,
} from "@/lib/apps-types";
import { cn } from "@/lib/utils";

type SortKey = "name" | "cpu" | "memory" | "restarts" | "uptime" | "rps" | "errors" | "p95";
type GroupBy = "none" | "kind" | "tag" | "domain";

const STATUS_DOT: Record<AppStatus, string> = {
  online: "bg-status-ok",
  stopped: "bg-muted-foreground",
  errored: "bg-status-critical",
  unknown: "bg-status-warning",
};

export function AppsPage() {
  const { servers, selectedServerId, statusById } = useServersContext();
  const server = servers.find((s) => s.id === selectedServerId);
  const connected =
    selectedServerId &&
    ["connected", "degraded"].includes(statusById.get(selectedServerId)?.status ?? "");
  const { apps, loading, error } = useApps();
  const [query, setQuery] = useState("");
  const [groupBy, setGroupBy] = useState<GroupBy>("kind");
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortAsc, setSortAsc] = useState(true);
  const [selected, setSelected] = useState<string[]>([]);
  const [editing, setEditing] = useState<TracelynApp | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const rows = apps.filter((app) => {
      if (!q) return true;
      return (
        app.name.toLowerCase().includes(q) ||
        app.domains.some((d) => d.toLowerCase().includes(q)) ||
        app.tags.some((t) => t.toLowerCase().includes(q)) ||
        app.kind.includes(q)
      );
    });
    rows.sort((a, b) => compareApps(a, b, sortKey) * (sortAsc ? 1 : -1));
    return rows;
  }, [apps, query, sortKey, sortAsc]);

  const groups = useMemo(() => groupApps(visible, groupBy), [visible, groupBy]);

  if (!server || !selectedServerId) {
    return (
      <EmptyState
        icon={Boxes}
        title="No server selected"
        description="Select a server in the sidebar to see the applications running on it."
      />
    );
  }

  const sortBy = (key: SortKey) => {
    if (sortKey === key) setSortAsc((v) => !v);
    else {
      setSortKey(key);
      setSortAsc(key === "name");
    }
  };

  const onMerge = async () => {
    if (selected.length < 2) return;
    setBusy(true);
    try {
      await mergeApps(selectedServerId, selected[0], selected.slice(1));
      setSelected([]);
      toast.success("Merged duplicate applications");
    } catch (err) {
      toast.error("Could not merge", { description: String(err) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-6">
      <OpsModeBanner />
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold tracking-tight">Applications</h2>
          <p className="text-sm text-muted-foreground">
            {server.name} · discovered every 60 seconds, plus anything you add by hand
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={!connected || busy}
            onClick={() => {
              void rediscoverApps(selectedServerId).then(
                () => toast.success("Rediscovery requested"),
                (err) => toast.error(String(err)),
              );
            }}
          >
            <RefreshCw data-icon="inline-start" />
            Rediscover
          </Button>
          <Button
            type="button"
            size="sm"
            onClick={() => {
              setEditing(blankApp(selectedServerId));
              setOpen(true);
            }}
          >
            <Plus data-icon="inline-start" />
            Add application
          </Button>
        </div>
      </div>

      {!connected && (
        <Notice tone="warning" title="Server is not connected">
          Connect from Overview to discover PM2, systemd, Docker, and web server apps.
        </Notice>
      )}
      {error && (
        <Notice tone="critical" title="Could not load applications">
          {error}
        </Notice>
      )}

      <div className="flex flex-wrap items-end gap-3">
        <div className="grid gap-1">
          <Label htmlFor="app-search">Search</Label>
          <Input
            id="app-search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Name, domain, or tag"
            className="w-64"
          />
        </div>
        <div className="grid gap-1">
          <Label htmlFor="app-group">Group</Label>
          <SimpleSelect
            id="app-group"
            value={groupBy}
            onValueChange={(next) => setGroupBy(next as GroupBy)}
            options={[
              { value: "none", label: "None" },
              { value: "kind", label: "Kind" },
              { value: "tag", label: "Tag" },
              { value: "domain", label: "Domain" },
            ]}
          />
        </div>
        <Button type="button" variant="outline" size="sm" disabled={selected.length < 2 || busy} onClick={() => void onMerge()}>
          Merge selected
        </Button>
      </div>

      {loading ? (
        <Skeleton className="h-64 w-full" />
      ) : visible.length === 0 ? (
        <EmptyState
          icon={Boxes}
          title="No applications yet"
          description="Tracelyn looks for PM2, custom systemd units, Docker, nginx, and Apache. You can also add one manually."
          action={
            <Button
              type="button"
              onClick={() => {
                setEditing(blankApp(selectedServerId));
                setOpen(true);
              }}
            >
              Add application
            </Button>
          }
        />
      ) : (
        groups.map(([label, rows]) => (
          <AppTable
            key={label}
            caption={label}
            rows={rows}
            selected={selected}
            onToggle={(id) =>
              setSelected((prev) => (prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id]))
            }
            onSort={sortBy}
            onEdit={(app) => {
              setEditing(app);
              setOpen(true);
            }}
            onHide={(app) => {
              void hideApp(selectedServerId, app.id, true).then(
                () => toast.success(`Hid ${app.name}`),
                (err) => toast.error(String(err)),
              );
            }}
          />
        ))
      )}

      <DiagnosticsPanel serverId={selectedServerId} />
      {editing && (
        <AppFormDialog
          key={editing.id}
          open={open}
          app={editing}
          onOpenChange={setOpen}
          onSave={async (app) => {
            await upsertManualApp(app);
            toast.success(`Saved ${app.name}`);
            setOpen(false);
          }}
        />
      )}
    </div>
  );
}

function AppTable({
  caption,
  rows,
  selected,
  onToggle,
  onSort,
  onEdit,
  onHide,
}: {
  caption: string;
  rows: TracelynApp[];
  selected: string[];
  onToggle: (id: string) => void;
  onSort: (key: SortKey) => void;
  onEdit: (app: TracelynApp) => void;
  onHide: (app: TracelynApp) => void;
}) {
  return (
    <div className="overflow-x-auto rounded-xl border border-border">
      <table className="w-full min-w-[960px] text-left text-sm">
        <caption className="px-3 py-2 text-left text-xs font-medium text-muted-foreground">
          {caption}
        </caption>
        <thead className="text-xs text-muted-foreground">
          <tr className="border-b border-border">
            <th className="w-8 px-2 py-2" />
            <th className="px-2 py-2">Status</th>
            <Th label="Name" onClick={() => onSort("name")} />
            <th className="px-2 py-2">Kind</th>
            <Th label="CPU" onClick={() => onSort("cpu")} />
            <Th label="Memory" onClick={() => onSort("memory")} />
            <Th label="Restarts" onClick={() => onSort("restarts")} />
            <Th label="Uptime" onClick={() => onSort("uptime")} />
            <Th label="req/s" onClick={() => onSort("rps")} />
            <Th label="Errors" onClick={() => onSort("errors")} />
            <Th label="p95" onClick={() => onSort("p95")} />
            <th className="px-2 py-2">Domains</th>
            <th className="px-2 py-2">Actions</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((app) => (
            <tr key={app.id} className="border-b border-border/60 last:border-0">
              <td className="px-2 py-2">
                <input
                  type="checkbox"
                  aria-label={`Select ${app.name}`}
                  checked={selected.includes(app.id)}
                  onChange={() => onToggle(app.id)}
                />
              </td>
              <td className="px-2 py-2">
                <span className={cn("inline-block size-2 rounded-full", STATUS_DOT[app.status])} />
                <span className="sr-only">{app.status}</span>
              </td>
              <td className="px-2 py-2 font-medium">
                <Link to={`/apps/${app.id}`} className="hover:underline">
                  {app.name}
                </Link>
              </td>
              <td className="px-2 py-2">{app.kind}</td>
              <td className="px-2 py-2 font-mono">{fmt(app.cpuPercent, "%")}</td>
              <td className="px-2 py-2 font-mono">{fmtBytes(app.memoryBytes)}</td>
              <td className="px-2 py-2 font-mono">{app.restarts ?? "—"}</td>
              <td className="px-2 py-2 font-mono">{fmtUptime(app.uptimeSecs)}</td>
              <td className="px-2 py-2 font-mono">{fmt(app.metrics?.reqPerSec, "")}</td>
              <td className="px-2 py-2 font-mono">{fmtRate(app.metrics?.errorRate)}</td>
              <td className="px-2 py-2 font-mono">{fmt(app.metrics?.p95Ms, " ms")}</td>
              <td className="max-w-48 truncate px-2 py-2">{app.domains.join(", ") || "—"}</td>
              <td className="px-2 py-2">
                <div className="flex flex-wrap items-center gap-2">
                  {(app.kind === "pm2" ||
                    app.kind === "systemd" ||
                    app.kind === "docker" ||
                    app.kind === "nginx" ||
                    app.kind === "apache") && (
                    <OpsQuickActions
                      serverId={app.serverId}
                      kind={app.kind}
                      target={app.name}
                      appId={app.id}
                      logPaths={app.logSources.flatMap((source) => (source.path ? [source.path] : []))}
                      verbs={app.kind === "nginx" || app.kind === "apache" ? ["reload"] : ["restart"]}
                      includeNav={false}
                      showHint={false}
                    />
                  )}
                  <Button type="button" variant="ghost" size="xs" onClick={() => onEdit(app)}>
                    Edit
                  </Button>
                  <Button type="button" variant="ghost" size="xs" onClick={() => onHide(app)}>
                    Hide
                  </Button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Th({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <th className="px-2 py-2">
      <button type="button" className="hover:underline" onClick={onClick}>
        {label}
      </button>
    </th>
  );
}

function AppFormDialog({
  open,
  app,
  onOpenChange,
  onSave,
}: {
  open: boolean;
  app: TracelynApp;
  onOpenChange: (open: boolean) => void;
  onSave: (app: TracelynApp) => Promise<void>;
}) {
  const [draft, setDraft] = useState(app);
  const [paths, setPaths] = useState(
    app.logSources.map((s) => s.path).filter(Boolean).join("\n"),
  );
  const [saving, setSaving] = useState(false);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form
          className="contents"
          onSubmit={(e) => {
            e.preventDefault();
            setSaving(true);
            const logSources = paths
              .split("\n")
              .map((p) => p.trim())
              .filter(Boolean)
              .map((path, i) => ({
                id: `custom-${i}`,
                kind: "custom" as LogSourceKind,
                path,
                readable: true,
              }));
            void onSave({ ...draft, origin: "manual", logSources, hidden: false }).finally(() =>
              setSaving(false),
            );
          }}
        >
          <DialogHeader>
            <DialogTitle>{app.origin === "manual" && app.name ? "Edit application" : "Add application"}</DialogTitle>
          </DialogHeader>
          <div className="grid gap-3">
            <div className="grid gap-1">
              <Label htmlFor="app-name">Name</Label>
              <Input
                id="app-name"
                required
                value={draft.name}
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              />
            </div>
            <div className="grid gap-1">
              <Label htmlFor="app-kind">Type</Label>
              <SimpleSelect
                id="app-kind"
                value={draft.kind}
                onValueChange={(next) => setDraft({ ...draft, kind: next as AppKind })}
                options={APP_KINDS.map((kind) => ({ value: kind, label: kind }))}
              />
            </div>
            <div className="grid gap-1">
              <Label htmlFor="app-logs">Log files (one path per line, globs allowed)</Label>
              <textarea
                id="app-logs"
                value={paths}
                onChange={(e) => setPaths(e.target.value)}
                rows={4}
                className="rounded-lg border border-input bg-transparent px-2 py-1 font-mono text-sm"
              />
            </div>
            <div className="grid gap-1">
              <Label htmlFor="app-port">Port</Label>
              <Input
                id="app-port"
                inputMode="numeric"
                value={draft.ports[0] ?? ""}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    ports: e.target.value ? [Number(e.target.value)] : [],
                  })
                }
              />
            </div>
            <div className="grid gap-1">
              <Label htmlFor="app-domain">Domain</Label>
              <Input
                id="app-domain"
                value={draft.domains.join(", ")}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    domains: e.target.value
                      .split(",")
                      .map((d) => d.trim())
                      .filter(Boolean),
                  })
                }
              />
            </div>
            <div className="grid gap-1">
              <Label htmlFor="app-tags">Tags</Label>
              <Input
                id="app-tags"
                value={draft.tags.join(", ")}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    tags: e.target.value
                      .split(",")
                      .map((d) => d.trim())
                      .filter(Boolean),
                  })
                }
              />
            </div>
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={saving}>
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function blankApp(serverId: string): TracelynApp {
  return {
    id: `manual-${crypto.randomUUID()}`,
    serverId,
    name: "",
    kind: "manual",
    status: "unknown",
    origin: "manual",
    hidden: false,
    pids: [],
    ports: [],
    domains: [],
    tags: [],
    logSources: [],
    linkedAppIds: [],
  };
}

function groupApps(rows: TracelynApp[], groupBy: GroupBy): [string, TracelynApp[]][] {
  if (groupBy === "none") return [["All applications", rows]];
  const map = new Map<string, TracelynApp[]>();
  for (const app of rows) {
    const keys =
      groupBy === "kind"
        ? [app.kind]
        : groupBy === "tag"
          ? app.tags.length ? app.tags : ["untagged"]
          : app.domains.length
            ? app.domains
            : ["no domain"];
    for (const key of keys) {
      const list = map.get(key) ?? [];
      list.push(app);
      map.set(key, list);
    }
  }
  return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
}

function compareApps(a: TracelynApp, b: TracelynApp, key: SortKey): number {
  const num = (v?: number | null) => v ?? -1;
  switch (key) {
    case "cpu":
      return num(a.cpuPercent) - num(b.cpuPercent);
    case "memory":
      return num(a.memoryBytes) - num(b.memoryBytes);
    case "restarts":
      return num(a.restarts) - num(b.restarts);
    case "uptime":
      return num(a.uptimeSecs) - num(b.uptimeSecs);
    case "rps":
      return num(a.metrics?.reqPerSec) - num(b.metrics?.reqPerSec);
    case "errors":
      return num(a.metrics?.errorRate) - num(b.metrics?.errorRate);
    case "p95":
      return num(a.metrics?.p95Ms) - num(b.metrics?.p95Ms);
    default:
      return a.name.localeCompare(b.name);
  }
}

function fmt(value: number | null | undefined, suffix: string) {
  if (value == null || Number.isNaN(value)) return "—";
  return `${value.toFixed(value >= 100 ? 0 : 1)}${suffix}`;
}

function fmtRate(value: number | null | undefined) {
  if (value == null) return "—";
  return `${(value * 100).toFixed(1)}%`;
}

function fmtBytes(value: number | null | undefined) {
  if (value == null) return "—";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(0)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function fmtUptime(secs: number | null | undefined) {
  if (secs == null) return "—";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (h > 48) return `${Math.floor(h / 24)}d`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}
