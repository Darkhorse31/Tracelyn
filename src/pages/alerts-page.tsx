import { useLicense } from "@/contexts/license-context";
import { useId, useState } from "react";
import {
  AlertTriangle,
  BellOff,
  BellRing,
  CircleCheck,
  History,
  Pencil,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import { AlertTemplates } from "@/components/intel/alert-templates";
import { OpsModeBanner } from "@/components/ops/operations-mode-card";
import { AlertQuickActions } from "@/components/ops/quick-actions";
import { Notice } from "@/components/status/notice";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { useAlerts } from "@/hooks/use-alerts";
import { useServersContext } from "@/contexts/servers-context";
import type { AlertRule } from "@/lib/alerts-types";

function formatTs(ms: number) {
  return new Date(ms).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function RuleEditor({
  rule,
  onSave,
  onCancel,
}: {
  rule: AlertRule;
  onSave: (rule: AlertRule) => Promise<void>;
  onCancel: () => void;
}) {
  const id = useId();
  const [draft, setDraft] = useState(rule);
  const [saving, setSaving] = useState(false);
  const invalid =
    !draft.name.trim() ||
    !Number.isFinite(draft.params.threshold) ||
    (draft.params.clearThreshold != null && !Number.isFinite(draft.params.clearThreshold)) ||
    !Number.isFinite(draft.params.durationSecs ?? 0) ||
    (draft.params.durationSecs ?? 0) < 0;

  const setParam = (key: "threshold" | "clearThreshold" | "durationSecs", raw: string) =>
    setDraft({
      ...draft,
      params: {
        ...draft.params,
        [key]: raw === "" ? (key === "clearThreshold" ? undefined : NaN) : Number(raw),
      },
    });

  return (
    <form
      aria-label={`Edit ${rule.name}`}
      className="grid gap-4 rounded-lg border border-border bg-muted/30 p-4 md:grid-cols-2"
      onSubmit={async (e) => {
        e.preventDefault();
        setSaving(true);
        try {
          await onSave({ ...draft, name: draft.name.trim(), updatedAtMs: Date.now() });
        } finally {
          setSaving(false);
        }
      }}
    >
      <div className="grid gap-2">
        <Label htmlFor={`${id}-name`}>Name</Label>
        <Input
          id={`${id}-name`}
          value={draft.name}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
        />
      </div>
      <div className="grid gap-2">
        <Label htmlFor={`${id}-threshold`}>Fire at or above</Label>
        <Input
          id={`${id}-threshold`}
          type="number"
          step="any"
          className="font-mono"
          value={Number.isNaN(draft.params.threshold) ? "" : draft.params.threshold}
          onChange={(e) => setParam("threshold", e.target.value)}
        />
      </div>
      <div className="grid gap-2">
        <Label htmlFor={`${id}-clear`}>Clear below (optional)</Label>
        <Input
          id={`${id}-clear`}
          type="number"
          step="any"
          className="font-mono"
          value={draft.params.clearThreshold ?? ""}
          onChange={(e) => setParam("clearThreshold", e.target.value)}
        />
      </div>
      <div className="grid gap-2">
        <Label htmlFor={`${id}-duration`}>Sustained for (seconds)</Label>
        <Input
          id={`${id}-duration`}
          type="number"
          min={0}
          className="font-mono"
          value={Number.isNaN(draft.params.durationSecs) ? "" : (draft.params.durationSecs ?? 0)}
          onChange={(e) => setParam("durationSecs", e.target.value)}
        />
      </div>
      <label className="flex min-h-6 w-fit cursor-pointer items-center gap-2 text-sm md:col-span-2">
        <input
          type="checkbox"
          checked={draft.enabled}
          onChange={(e) => setDraft({ ...draft, enabled: e.target.checked })}
          className="size-4 accent-primary"
        />
        Enabled
      </label>
      <div className="flex gap-2 md:col-span-2">
        <Button type="submit" size="sm" disabled={invalid || saving}>
          Save rule
        </Button>
        <Button type="button" size="sm" variant="outline" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}

function RuleSummary({ rule }: { rule: AlertRule }) {
  const { threshold, clearThreshold, durationSecs } = rule.params;
  return (
    <p className="font-mono text-xs text-muted-foreground">
      fire ≥ {threshold}
      {clearThreshold != null && ` · clear < ${clearThreshold}`}
      {durationSecs ? ` · for ${durationSecs} s` : ""}
    </p>
  );
}

export function AlertsPage() {
  const { license } = useLicense();
  const { selectedServerId, servers } = useServersContext();
  const { rules, history, loading, error, saveRule, removeRule } = useAlerts(
    selectedServerId ?? undefined,
  );
  const [editingId, setEditingId] = useState<string | null>(null);
  const [historyQuery, setHistoryQuery] = useState("");
  const [historyState, setHistoryState] = useState<"all" | "active" | "resolved">("all");
  const [pendingDelete, setPendingDelete] = useState<AlertRule | null>(null);
  const [deleting, setDeleting] = useState(false);
  const serverName = (id: string) => servers.find((s) => s.id === id)?.name ?? "Unknown server";

  const confirmDelete = async () => {
    if (!pendingDelete) return;
    setDeleting(true);
    try {
      await removeRule(pendingDelete.id);
      toast.success(`Deleted rule "${pendingDelete.name}"`);
      setPendingDelete(null);
    } catch (err) {
      toast.error("Could not delete rule", { description: String(err) });
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="flex flex-col gap-6">
      <OpsModeBanner />
      <fieldset disabled={!license?.canWrite}><AlertTemplates /></fieldset>
      {error && (
        <Notice tone="critical" title="Could not load alerts">
          {error}
        </Notice>
      )}
      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-sm">
              <AlertTriangle className="size-4 text-muted-foreground" aria-hidden />
              Alert rules
            </CardTitle>
            <CardDescription className="text-xs">
              Rules fire after the threshold is held for the duration and clear below the
              clear threshold, which reduces flapping.
            </CardDescription>
          </CardHeader>
          <CardContent>
            {loading ? (
              <div className="flex flex-col gap-2">
                {Array.from({ length: 4 }, (_, i) => (
                  <Skeleton key={i} className="h-[74px] w-full rounded-lg" />
                ))}
              </div>
            ) : rules.length === 0 ? (
              <p className="text-sm text-muted-foreground">No alert rules defined.</p>
            ) : (
              <ul className="flex flex-col gap-2">
                {rules.map((rule) => (
                  <li key={rule.id}>
                    {editingId === rule.id ? (
                      <RuleEditor
                        rule={rule}
                        onSave={async (r) => {
                          try {
                            await saveRule(r);
                            toast.success(`Saved rule "${r.name}"`);
                            setEditingId(null);
                          } catch (err) {
                            toast.error("Could not save rule", { description: String(err) });
                          }
                        }}
                        onCancel={() => setEditingId(null)}
                      />
                    ) : (
                      <div className="flex items-center justify-between gap-4 rounded-lg border border-border px-4 py-3">
                        <div className="flex min-w-0 flex-col gap-1">
                          <div className="flex flex-wrap items-center gap-2">
                            <h3 className="truncate text-sm font-medium">{rule.name}</h3>
                            <Badge variant={rule.enabled ? "secondary" : "outline"}>
                              {rule.enabled ? <BellRing aria-hidden /> : <BellOff aria-hidden />}
                              {rule.enabled ? "Enabled" : "Disabled"}
                            </Badge>
                          </div>
                          <RuleSummary rule={rule} />
                        </div>
                        <div className="flex shrink-0 gap-1">
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            aria-label={`Edit ${rule.name}`}
                            disabled={!license?.canWrite}
                            onClick={() => setEditingId(rule.id)}
                          >
                            <Pencil />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            aria-label={`Delete ${rule.name}`}
                            disabled={!license?.canWrite}
                            onClick={() => setPendingDelete(rule)}
                          >
                            <Trash2 />
                          </Button>
                        </div>
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2 text-sm">
              <History className="size-4 text-muted-foreground" aria-hidden />
              Alert history
            </CardTitle>
            <CardDescription className="text-xs">
              {selectedServerId
                ? `Events for ${serverName(selectedServerId)}.`
                : "Recent events across all servers."}
            </CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-3">
            <div className="flex flex-wrap gap-2">
              <Input
                aria-label="Filter history"
                placeholder="Filter by rule or message"
                value={historyQuery}
                onChange={(event) => setHistoryQuery(event.target.value)}
              />
              {(["all", "active", "resolved"] as const).map((state) => (
                <Button key={state} type="button" size="sm" variant={historyState === state ? "default" : "outline"} onClick={() => setHistoryState(state)}>
                  {state === "all" ? "All" : state === "active" ? "Active" : "Resolved"}
                </Button>
              ))}
            </div>
            {loading ? (
              <Skeleton className="h-48 w-full" />
            ) : history.length === 0 ? (
              <p className="text-sm text-muted-foreground">No alerts have fired yet.</p>
            ) : (
              <ScrollArea className="h-[min(420px,50vh)] pr-3">
                <ul className="flex flex-col gap-2">
                  {history
                    .filter((evt) => {
                      const active = !evt.resolvedAtMs;
                      if (historyState === "active" && !active) return false;
                      if (historyState === "resolved" && active) return false;
                      const q = historyQuery.trim().toLowerCase();
                      if (!q) return true;
                      return `${evt.ruleName} ${evt.message}`.toLowerCase().includes(q);
                    })
                    .map((evt) => {
                    const active = !evt.resolvedAtMs;
                    return (
                      <li key={evt.id} className="rounded-lg border border-border px-4 py-3">
                        <div className="flex items-start justify-between gap-2">
                          <h3 className="text-sm font-medium">{evt.ruleName}</h3>
                          {active ? (
                            <Badge className="bg-status-critical text-status-critical-foreground">
                              <BellRing aria-hidden />
                              Active
                            </Badge>
                          ) : (
                            <Badge variant="secondary">
                              <CircleCheck aria-hidden />
                              Resolved
                            </Badge>
                          )}
                        </div>
                        <p className="mt-1 text-sm text-muted-foreground">{evt.message}</p>
                        <p className="mt-2 font-mono text-xs text-muted-foreground">
                          {!selectedServerId && `${serverName(evt.serverId)} · `}
                          {formatTs(evt.firedAtMs)}
                          {evt.resolvedAtMs && ` → ${formatTs(evt.resolvedAtMs)}`}
                        </p>
                        <AlertQuickActions serverId={evt.serverId} />
                      </li>
                    );
                  })}
                </ul>
              </ScrollArea>
            )}
          </CardContent>
        </Card>
      </div>

      <Dialog open={pendingDelete != null} onOpenChange={(open) => !open && setPendingDelete(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete alert rule?</DialogTitle>
            <DialogDescription>
              “{pendingDelete?.name}” will stop firing. Past events stay in the history.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setPendingDelete(null)}>
              Cancel
            </Button>
            <Button variant="destructive" disabled={!license?.canWrite || deleting} onClick={() => void confirmDelete()}>
              Delete rule
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
