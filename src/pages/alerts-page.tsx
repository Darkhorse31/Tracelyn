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
import { alertTestSend } from "@/lib/alerts-api";
import type { AlertRule, RuleKind } from "@/lib/alerts-types";
import { usePrivacy } from "@/contexts/privacy-context";

const KINDS: { value: RuleKind; label: string }[] = [
  { value: "cpu_high", label: "CPU" },
  { value: "memory_high", label: "Memory" },
  { value: "disk_high", label: "Disk" },
  { value: "http_5xx_rate", label: "HTTP 5xx rate" },
  { value: "endpoint_p95", label: "Endpoint p95" },
  { value: "server_unreachable", label: "Server unreachable" },
  { value: "redis_memory_near_max", label: "Redis memory" },
  { value: "log_query", label: "Log query" },
  { value: "trace_p95", label: "Trace p95" },
  { value: "trace_error_rate", label: "Trace error rate" },
];

function minutesToTime(min?: number | null): string {
  if (min == null) return "";
  const hours = Math.floor(min / 60) % 24;
  const minutes = min % 60;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
}

function timeToMinutes(value: string): number | undefined {
  if (!value) return undefined;
  const [hours, minutes] = value.split(":").map(Number);
  if (!Number.isFinite(hours) || !Number.isFinite(minutes)) return undefined;
  return hours * 60 + minutes;
}

function blankRule(): AlertRule {
  return {
    id: crypto.randomUUID(),
    kind: "log_query",
    name: "",
    enabled: true,
    params: { threshold: 1, durationSecs: 0, windowSecs: 60, cooldownSecs: 300, conditions: [] },
    createdAtMs: Date.now(),
    updatedAtMs: Date.now(),
  };
}

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
  const conditions = draft.params.conditions ?? [];
  const invalid =
    !draft.name.trim() ||
    !Number.isFinite(draft.params.threshold) ||
    (draft.params.clearThreshold != null && !Number.isFinite(draft.params.clearThreshold)) ||
    !Number.isFinite(draft.params.durationSecs ?? 0) ||
    (draft.params.durationSecs ?? 0) < 0 ||
    (draft.kind === "log_query" && !draft.params.logQuery?.trim()) ||
    conditions.some((condition) => !condition.metricKey.trim() || !Number.isFinite(condition.threshold));

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
        <Label htmlFor={`${id}-kind`}>Kind</Label>
        <select
          id={`${id}-kind`}
          className="h-8 rounded-lg border border-input bg-transparent px-2 text-sm"
          value={draft.kind}
          onChange={(e) => setDraft({ ...draft, kind: e.target.value as RuleKind })}
        >
          {KINDS.map((kind) => (
            <option key={kind.value} value={kind.value}>
              {kind.label}
            </option>
          ))}
        </select>
      </div>
      <div className="grid gap-2">
        <Label htmlFor={`${id}-duration`}>For (seconds)</Label>
        <Input
          id={`${id}-duration`}
          type="number"
          min={0}
          className="font-mono"
          value={Number.isNaN(draft.params.durationSecs) ? "" : (draft.params.durationSecs ?? 0)}
          onChange={(e) => setParam("durationSecs", e.target.value)}
        />
      </div>
      <div className="grid gap-2">
        <Label htmlFor={`${id}-cooldown`}>Cooldown (seconds)</Label>
        <Input
          id={`${id}-cooldown`}
          type="number"
          min={0}
          className="font-mono"
          value={draft.params.cooldownSecs ?? 0}
          onChange={(e) =>
            setDraft({
              ...draft,
              params: { ...draft.params, cooldownSecs: Number(e.target.value) },
            })
          }
        />
      </div>
      <div className="grid gap-2">
        <Label htmlFor={`${id}-window`}>Log window (seconds)</Label>
        <Input
          id={`${id}-window`}
          type="number"
          min={1}
          className="font-mono"
          value={draft.params.windowSecs ?? 60}
          onChange={(e) =>
            setDraft({
              ...draft,
              params: { ...draft.params, windowSecs: Number(e.target.value) },
            })
          }
        />
      </div>
      <div className="grid gap-2">
        <Label htmlFor={`${id}-quiet-start`}>Quiet hours start</Label>
        <Input
          id={`${id}-quiet-start`}
          type="time"
          value={minutesToTime(draft.params.quietStartMin)}
          onChange={(e) =>
            setDraft({
              ...draft,
              params: { ...draft.params, quietStartMin: timeToMinutes(e.target.value) ?? null },
            })
          }
        />
      </div>
      <div className="grid gap-2">
        <Label htmlFor={`${id}-quiet-end`}>Quiet hours end</Label>
        <Input
          id={`${id}-quiet-end`}
          type="time"
          value={minutesToTime(draft.params.quietEndMin)}
          onChange={(e) =>
            setDraft({
              ...draft,
              params: { ...draft.params, quietEndMin: timeToMinutes(e.target.value) ?? null },
            })
          }
        />
      </div>
      {draft.kind === "log_query" && (
        <div className="grid gap-2 md:col-span-2">
          <Label htmlFor={`${id}-query`}>Log query</Label>
          <Input
            id={`${id}-query`}
            className="font-mono"
            value={draft.params.logQuery ?? ""}
            placeholder={'level:error "payment failed"'}
            onChange={(e) =>
              setDraft({ ...draft, params: { ...draft.params, logQuery: e.target.value } })
            }
          />
        </div>
      )}
      {(draft.kind === "trace_p95" || draft.kind === "trace_error_rate") && (
        <div className="grid gap-2 md:col-span-2">
          <Label htmlFor={`${id}-endpoint`}>Endpoint</Label>
          <Input
            id={`${id}-endpoint`}
            className="font-mono"
            value={draft.params.endpoint ?? ""}
            placeholder="GET /api/orders"
            onChange={(e) =>
              setDraft({ ...draft, params: { ...draft.params, endpoint: e.target.value } })
            }
          />
        </div>
      )}
      <div className="flex flex-col gap-2 md:col-span-2">
        <div className="flex items-center justify-between gap-2">
          <span className="text-sm font-medium">Conditions</span>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() =>
              setDraft({
                ...draft,
                params: {
                  ...draft.params,
                  conditions: [...conditions, { metricKey: "cpu.percent", op: "gte", threshold: 80 }],
                },
              })
            }
          >
            Add condition
          </Button>
        </div>
        {conditions.map((condition, index) => (
          <div key={`${condition.metricKey}-${index}`} className="grid grid-cols-[1fr_5rem_6rem_auto] items-center gap-2">
            <Input
              aria-label={`Condition ${index + 1} metric`}
              className="font-mono"
              value={condition.metricKey}
              onChange={(e) => {
                const next = conditions.slice();
                next[index] = { ...condition, metricKey: e.target.value };
                setDraft({ ...draft, params: { ...draft.params, conditions: next } });
              }}
            />
            <select
              aria-label={`Condition ${index + 1} operator`}
              className="h-8 rounded-lg border border-input bg-transparent px-2 text-sm"
              value={condition.op}
              onChange={(e) => {
                const next = conditions.slice();
                next[index] = { ...condition, op: e.target.value };
                setDraft({ ...draft, params: { ...draft.params, conditions: next } });
              }}
            >
              <option value="gte">≥</option>
              <option value="lte">≤</option>
              <option value="gt">&gt;</option>
              <option value="lt">&lt;</option>
            </select>
            <Input
              aria-label={`Condition ${index + 1} threshold`}
              type="number"
              className="font-mono"
              value={condition.threshold}
              onChange={(e) => {
                const next = conditions.slice();
                next[index] = { ...condition, threshold: Number(e.target.value) };
                setDraft({ ...draft, params: { ...draft.params, conditions: next } });
              }}
            />
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => {
                const next = conditions.filter((_, item) => item !== index);
                setDraft({ ...draft, params: { ...draft.params, conditions: next } });
              }}
            >
              Remove
            </Button>
          </div>
        ))}
      </div>
      <label className="flex min-h-6 w-fit cursor-pointer items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={Boolean(draft.params.alertOnNoData)}
          onChange={(e) =>
            setDraft({ ...draft, params: { ...draft.params, alertOnNoData: e.target.checked } })
          }
          className="size-4 accent-primary"
        />
        Alert when there is no data
      </label>
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
  const { threshold, clearThreshold, durationSecs, cooldownSecs, conditions } = rule.params;
  return (
    <p className="font-mono text-xs text-muted-foreground">
      fire ≥ {threshold}
      {clearThreshold != null && ` · clear < ${clearThreshold}`}
      {durationSecs ? ` · for ${durationSecs} s` : ""}
      {cooldownSecs ? ` · cooldown ${cooldownSecs} s` : ""}
      {conditions && conditions.length > 0 ? ` · ${conditions.length} conditions` : ""}
      {rule.params.logQuery ? ` · ${rule.params.logQuery}` : ""}
      {rule.params.endpoint ? ` · ${rule.params.endpoint}` : ""}
    </p>
  );
}

export function AlertsPage() {
  const { license } = useLicense();
  const { mask } = usePrivacy();
  const { selectedServerId, servers } = useServersContext();
  const { rules, history, loading, error, saveRule, removeRule, silenceRule } = useAlerts(
    selectedServerId ?? undefined,
  );
  const [editingId, setEditingId] = useState<string | null>(null);
  const [creating, setCreating] = useState<AlertRule | null>(null);
  const [testing, setTesting] = useState(false);
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
      <Notice tone="info" title="Alerts run only while Tracelyn is open">
        Rules are checked on this computer. Close the app and they stop firing until you open it again.
      </Notice>
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
              A rule fires after the value stays at or above the threshold for the duration you set.
              Quiet hours, cooldown, and silence hold the notification. The check still runs.
            </CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-3">
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                size="sm"
                disabled={!license?.canWrite}
                onClick={() => {
                  setEditingId(null);
                  setCreating(blankRule());
                }}
              >
                New monitor
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={!license?.canWrite || testing}
                onClick={async () => {
                  setTesting(true);
                  try {
                    toast.success(await alertTestSend());
                  } catch (err) {
                    toast.error(String(err));
                  } finally {
                    setTesting(false);
                  }
                }}
              >
                Send test
              </Button>
            </div>
            {creating && (
              <RuleEditor
                rule={creating}
                onSave={async (r) => {
                  try {
                    await saveRule(r);
                    toast.success(`Saved rule "${r.name}"`);
                    setCreating(null);
                  } catch (err) {
                    toast.error("Could not save rule", { description: String(err) });
                  }
                }}
                onCancel={() => setCreating(null)}
              />
            )}
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
                            {rule.firing && <Badge>Firing</Badge>}
                            {rule.noData && <Badge variant="outline">No data</Badge>}
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
                            size="sm"
                            disabled={!license?.canWrite}
                            onClick={async () => {
                              try {
                                await silenceRule(rule.id);
                                toast.success(`Silenced "${rule.name}" for 1 hour`);
                              } catch (err) {
                                toast.error(String(err));
                              }
                            }}
                          >
                            Silence for 1 hour
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
                        <p className="mt-1 text-sm text-muted-foreground">{mask(evt.message)}</p>
                        <p className="mt-2 font-mono text-xs text-muted-foreground">
                          {!selectedServerId && `${serverName(evt.serverId)} · `}
                          {formatTs(evt.firedAtMs)}
                          {evt.resolvedAtMs && ` → ${formatTs(evt.resolvedAtMs)}`}
                        </p>
                        <AlertQuickActions serverId={evt.serverId} />
                        {active && (
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            className="mt-2"
                            disabled={!license?.canWrite}
                            onClick={async () => {
                              try {
                                await silenceRule(evt.ruleId);
                                toast.success("Silenced for 1 hour");
                              } catch (err) {
                                toast.error(String(err));
                              }
                            }}
                          >
                            Silence for 1 hour
                          </Button>
                        )}
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
