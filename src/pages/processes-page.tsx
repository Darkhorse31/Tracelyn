import { useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Cpu } from "lucide-react";
import { OpsConfirmDialog } from "@/components/ops/confirm-action-dialog";
import { EmptyState } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Masked } from "@/contexts/privacy-context";
import { useLicense } from "@/contexts/license-context";
import { useServersContext } from "@/contexts/servers-context";
import { useProcesses } from "@/hooks/use-processes";
import { useRememberedState } from "@/hooks/use-remembered-state";
import type { ActionRequest } from "@/lib/ops-types";
import type { ProcessTreeRow } from "@/lib/types";

type Row =
  | { kind: "group"; key: string; label: string; count: number }
  | { kind: "proc"; key: string; depth: number; row: ProcessTreeRow };

export function ProcessesPage() {
  const { servers, selectedServerId, statusById } = useServersContext();
  const server = servers.find((item) => item.id === selectedServerId);
  const connected =
    selectedServerId &&
    ["connected", "degraded"].includes(statusById.get(selectedServerId)?.status ?? "");
  const { rows, waiting } = useProcesses(selectedServerId, Boolean(connected));
  const [grouped, setGrouped] = useRememberedState("processes:grouped", true);
  const [action, setAction] = useState<ActionRequest | null>(null);
  const { license } = useLicense();
  const canWrite = Boolean(license?.canWrite);
  const parentRef = useRef<HTMLDivElement>(null);

  const items = useMemo(() => flatten(rows, grouped), [rows, grouped]);
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => (items[index]?.kind === "group" ? 36 : 40),
    overscan: 12,
  });

  if (!server || !selectedServerId) {
    return (
      <EmptyState
        icon={Cpu}
        title="No server selected"
        description="Select a server to inspect its process tree."
      />
    );
  }

  return (
    <div className="flex h-[calc(100svh-7rem)] min-h-0 flex-col gap-3">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold tracking-tight">Processes</h2>
          <p className="text-sm text-muted-foreground">
            {server.name}
            {connected ? "" : " is not connected. The tree appears after you connect."}
          </p>
        </div>
        <Button
          type="button"
          variant={grouped ? "secondary" : "outline"}
          aria-pressed={grouped}
          onClick={() => setGrouped((value) => !value)}
        >
          Group by service
        </Button>
      </div>
      {!canWrite && (
        <p className="text-sm text-muted-foreground">A read-only license disables process actions.</p>
      )}
      <div
        ref={parentRef}
        className="min-h-0 flex-1 overflow-auto rounded-lg border border-border bg-background"
      >
        {waiting && rows.length === 0 ? (
          <p className="p-6 text-sm text-muted-foreground">Waiting for the first process sample.</p>
        ) : items.length === 0 ? (
          <p className="p-6 text-sm text-muted-foreground">No processes reported yet.</p>
        ) : (
          <div className="min-w-[68rem]">
            <div className="sticky top-0 z-10 grid grid-cols-[4.5rem_6rem_3.5rem_4.5rem_3.5rem_4.5rem_minmax(0,1fr)_auto] gap-2 border-b border-border bg-background px-3 py-2 text-xs font-medium text-muted-foreground">
              <span>PID</span>
              <span>User</span>
              <span className="text-right">CPU</span>
              <span className="text-right">RSS</span>
              <span className="text-right">Threads</span>
              <span>Trend</span>
              <span>Command</span>
              <span className="text-right">Actions</span>
            </div>
            <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((item) => {
              const row = items[item.index];
              return (
                <div
                  key={row.key}
                  data-index={item.index}
                  ref={virtualizer.measureElement}
                  className="absolute left-0 w-full border-b border-border/60"
                  style={{ transform: `translateY(${item.start}px)` }}
                >
                  {row.kind === "group" ? (
                    <div className="flex items-center justify-between bg-muted/50 px-3 py-2 text-xs font-medium text-foreground">
                      <span>{row.label}</span>
                      <span className="tabular-nums text-muted-foreground">{row.count}</span>
                    </div>
                  ) : (
                    <ProcessLine
                      row={row.row}
                      depth={row.depth}
                      canWrite={canWrite}
                      serverId={selectedServerId}
                      onAction={setAction}
                    />
                  )}
                </div>
              );
            })}
            </div>
          </div>
        )}
      </div>
      <OpsConfirmDialog open={action != null} onOpenChange={(open) => !open && setAction(null)} request={action} />
    </div>
  );
}

function ProcessLine({
  row,
  depth,
  canWrite,
  serverId,
  onAction,
}: {
  row: ProcessTreeRow;
  depth: number;
  canWrite: boolean;
  serverId: string;
  onAction: (request: ActionRequest) => void;
}) {
  const restartKind = row.serviceKind === "pm2" || row.serviceKind === "systemd" || row.serviceKind === "docker"
    ? row.serviceKind
    : null;
  return (
    <div className="grid grid-cols-[4.5rem_6rem_3.5rem_4.5rem_3.5rem_4.5rem_minmax(0,1fr)_auto] items-center gap-2 px-3 py-1.5 text-xs">
      <span className="font-mono tabular-nums" style={{ paddingLeft: Math.min(depth, 6) * 12 }}>
        {row.pid}
      </span>
      <span className="truncate text-muted-foreground">
        <Masked text={row.user} />
      </span>
      <span className="text-right font-mono tabular-nums">{row.cpuPercent.toFixed(1)}%</span>
      <span className="text-right font-mono tabular-nums">{formatRss(row.rssKb)}</span>
      <span className="text-right font-mono tabular-nums">{row.threads}</span>
      <Spark values={row.cpuSparkline} />
      <span className="min-w-0">
        <span className="block truncate font-mono" title={row.command}>
          <Masked text={row.command} />
        </span>
        <span className="block truncate text-muted-foreground">
          {row.service ? `${row.serviceKind ?? "service"} ${row.service}` : "No service"}
          {" · "}
          {row.startedMs ? new Date(row.startedMs).toLocaleString() : "Unknown start"}
        </span>
      </span>
      <span className="flex gap-1">
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={!canWrite}
          onClick={() =>
            onAction({
              serverId,
              kind: "process",
              verb: "stop",
              target: String(row.pid),
              useSudo: true,
            })
          }
        >
          Send SIGTERM
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={!canWrite || !restartKind || !row.service}
          onClick={() => {
            if (!restartKind || !row.service) return;
            onAction({
              serverId,
              kind: restartKind,
              verb: "restart",
              target: row.service,
              useSudo: restartKind !== "pm2",
            });
          }}
        >
          Restart service
        </Button>
      </span>
    </div>
  );
}

function Spark({ values }: { values: number[] }) {
  if (values.length < 2) return <span className="text-muted-foreground">—</span>;
  const width = 64;
  const height = 16;
  const max = Math.max(...values, 1);
  const points = values
    .map((value, index) => {
      const x = (index / (values.length - 1)) * width;
      const y = height - (value / max) * (height - 1);
      return `${x},${y}`;
    })
    .join(" ");
  return (
    <svg width={width} height={height} aria-hidden className="text-foreground">
      <polyline fill="none" stroke="currentColor" strokeWidth="1.25" points={points} />
    </svg>
  );
}

function formatRss(kb: number) {
  if (kb >= 1024 * 1024) return `${(kb / 1024 / 1024).toFixed(1)} GB`;
  if (kb >= 1024) return `${(kb / 1024).toFixed(1)} MB`;
  return `${kb} KB`;
}

function flatten(rows: ProcessTreeRow[], grouped: boolean): Row[] {
  const tree = treeRows(rows);
  if (!grouped) return tree.map((item) => ({ kind: "proc", key: `p-${item.row.pid}`, ...item }));
  const buckets = new Map<string, { label: string; items: { depth: number; row: ProcessTreeRow }[] }>();
  for (const item of tree) {
    const label = item.row.service
      ? `${item.row.serviceKind ?? "service"}: ${item.row.service}`
      : "Ungrouped";
    const bucket = buckets.get(label) ?? { label, items: [] };
    bucket.items.push(item);
    buckets.set(label, bucket);
  }
  const out: Row[] = [];
  for (const bucket of [...buckets.values()].sort((a, b) => a.label.localeCompare(b.label))) {
    out.push({ kind: "group", key: `g-${bucket.label}`, label: bucket.label, count: bucket.items.length });
    for (const item of bucket.items) {
      out.push({ kind: "proc", key: `p-${bucket.label}-${item.row.pid}`, depth: item.depth, row: item.row });
    }
  }
  return out;
}

function treeRows(rows: ProcessTreeRow[]) {
  const ids = new Set(rows.map((row) => row.pid));
  const children = new Map<number, ProcessTreeRow[]>();
  for (const row of rows) {
    const parent = ids.has(row.ppid) ? row.ppid : 0;
    const list = children.get(parent) ?? [];
    list.push(row);
    children.set(parent, list);
  }
  for (const list of children.values()) list.sort((a, b) => b.cpuPercent - a.cpuPercent);
  const out: { depth: number; row: ProcessTreeRow }[] = [];
  const walk = (pid: number, depth: number) => {
    for (const row of children.get(pid) ?? []) {
      out.push({ depth, row });
      walk(row.pid, depth + 1);
    }
  };
  walk(0, 0);
  return out;
}
