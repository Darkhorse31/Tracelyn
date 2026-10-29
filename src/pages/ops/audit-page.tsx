import { useCallback, useEffect, useId, useState } from "react";
import { ClipboardList } from "lucide-react";
import { toast } from "sonner";
import { OpsModeBanner } from "@/components/ops/operations-mode-card";
import { PageHeader } from "@/components/layout/page-header";
import { EmptyState, Notice } from "@/components/status/notice";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { SimpleSelect } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useServersContext } from "@/contexts/servers-context";
import { opsExportAudit, opsListAudit } from "@/lib/ops-api";
import type { AuditRow } from "@/lib/ops-types";
import { useRememberedState } from "@/hooks/use-remembered-state";

function localInputToMs(value: string): number | undefined {
  if (!value) return undefined;
  const ms = new Date(value).getTime();
  return Number.isNaN(ms) ? undefined : ms;
}

export function AuditPage() {
  const id = useId();
  const { servers } = useServersContext();
  const [serverId, setServerId] = useRememberedState("audit:server", "");
  const [target, setTarget] = useRememberedState("audit:target", "");
  const [since, setSince] = useRememberedState("audit:since", "");
  const [until, setUntil] = useRememberedState("audit:until", "");
  const [failedOnly, setFailedOnly] = useRememberedState("audit:failed-only", false);
  const [rows, setRows] = useState<AuditRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const next = await opsListAudit({
        serverId: serverId || undefined,
        target: target.trim() || undefined,
        sinceMs: localInputToMs(since),
        untilMs: localInputToMs(until),
        failedOnly,
        limit: 200,
      });
      setRows(next);
      setError(null);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [serverId, target, since, until, failedOnly]);

  useEffect(() => {
    void load();
  }, [load]);

  const exportCsv = async () => {
    try {
      const csv = await opsExportAudit({
        serverId: serverId || undefined,
        target: target.trim() || undefined,
        sinceMs: localInputToMs(since),
        untilMs: localInputToMs(until),
        failedOnly,
        limit: 500,
      });
      const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = "tracelyn-audit.csv";
      link.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      toast.error("Could not export the audit log", { description: String(err) });
    }
  };

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title="Audit"
        description="Who ran which command, on which server, and how it exited. Output is stored as a short excerpt."
        actions={
          <Button type="button" variant="outline" onClick={() => void exportCsv()}>
            Export CSV
          </Button>
        }
      />
      <OpsModeBanner />
      <form
        className="grid items-end gap-4 md:grid-cols-2 xl:grid-cols-5"
        onSubmit={(event) => {
          event.preventDefault();
          void load();
        }}
      >
        <div className="grid gap-2">
          <Label htmlFor={`${id}-server`}>Server</Label>
          <SimpleSelect
            id={`${id}-server`}
            value={serverId || "__all__"}
            onValueChange={(next) => setServerId(next === "__all__" ? "" : next)}
            options={[
              { value: "__all__", label: "All servers" },
              ...servers.map((server) => ({ value: server.id, label: server.name })),
            ]}
          />
        </div>
        <div className="grid gap-2">
          <Label htmlFor={`${id}-target`}>Target contains</Label>
          <Input id={`${id}-target`} value={target} onChange={(event) => setTarget(event.target.value)} />
        </div>
        <div className="grid gap-2">
          <Label htmlFor={`${id}-since`}>From</Label>
          <Input id={`${id}-since`} type="datetime-local" value={since} onChange={(event) => setSince(event.target.value)} />
        </div>
        <div className="grid gap-2">
          <Label htmlFor={`${id}-until`}>To</Label>
          <Input id={`${id}-until`} type="datetime-local" value={until} onChange={(event) => setUntil(event.target.value)} />
        </div>
        <label className="flex items-end gap-2 pb-2 text-sm" htmlFor={`${id}-failed`}>
          <input
            id={`${id}-failed`}
            type="checkbox"
            className="size-4 accent-primary"
            checked={failedOnly}
            onChange={(event) => setFailedOnly(event.target.checked)}
          />
          Failed only
        </label>
      </form>

      {error && (
        <Notice tone="critical" title="Could not load the audit log">
          {error}
        </Notice>
      )}
      {loading ? (
        <Skeleton className="h-48 w-full" />
      ) : rows.length === 0 ? (
        <EmptyState
          icon={ClipboardList}
          title="No operations recorded"
          description="Confirmed actions appear here with the Windows user, server, command, exit code, and a short output excerpt."
        />
      ) : (
        <div className="overflow-x-auto rounded-xl border border-border">
          <table className="w-full min-w-[880px] text-left text-sm">
            <caption className="px-3 py-2 text-left text-xs font-medium text-muted-foreground">
              {rows.length} audit {rows.length === 1 ? "row" : "rows"}
            </caption>
            <thead className="text-xs text-muted-foreground">
              <tr className="border-b border-border">
                <th scope="col" className="px-3 py-2 font-medium">Time</th>
                <th scope="col" className="px-3 py-2 font-medium">Who</th>
                <th scope="col" className="px-3 py-2 font-medium">Server</th>
                <th scope="col" className="px-3 py-2 font-medium">Target</th>
                <th scope="col" className="px-3 py-2 font-medium">Command</th>
                <th scope="col" className="px-3 py-2 font-medium">Exit</th>
                <th scope="col" className="px-3 py-2 font-medium">Output</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => {
                const failed = row.exitCode == null || row.exitCode !== 0;
                return (
                  <tr key={row.id} className="border-b border-border/60 align-top last:border-0">
                    <td className="px-3 py-2 whitespace-nowrap font-mono text-xs">
                      {new Date(row.tsMs).toLocaleString()}
                    </td>
                    <td className="px-3 py-2">{row.actor}</td>
                    <td className="px-3 py-2">{row.serverName}</td>
                    <td className="px-3 py-2 font-mono text-xs">{row.target}</td>
                    <td className="max-w-xs px-3 py-2">
                      <pre className="font-mono text-xs whitespace-pre-wrap" title={row.command}>
                        {row.command}
                      </pre>
                    </td>
                    <td className="px-3 py-2">
                      <Badge variant={failed ? "destructive" : "secondary"}>
                        {failed ? "Failed" : "OK"} {row.exitCode ?? "—"}
                      </Badge>
                    </td>
                    <td className="max-w-xs px-3 py-2 text-xs text-muted-foreground whitespace-pre-wrap">
                      {row.outputExcerpt || "—"}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
