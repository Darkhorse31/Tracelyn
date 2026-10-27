import { useEffect, useId, useState } from "react";
import { Pencil, Play, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { EmptyState, Notice } from "@/components/status/notice";
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
import { Masked, SecretInput } from "@/contexts/privacy-context";
import { Label } from "@/components/ui/label";
import { SimpleSelect } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { AlertTemplates } from "@/components/intel/alert-templates";
import { SeverityBadge } from "@/components/intel/format";
import { useServersContext } from "@/contexts/servers-context";
import { deleteIntelRule, evaluateIntel, listIntelRules, upsertIntelRule } from "@/pages/intel/api";
import {
  emptyParams,
  KIND_LABELS,
  METRIC_KEYS,
  type IntelKind,
  type IntelRule,
  type Severity,
} from "@/pages/intel/types";

const KINDS = Object.keys(KIND_LABELS) as IntelKind[];

function blank(kind: IntelKind = "metric"): IntelRule {
  return {
    id: "",
    serverId: null,
    kind,
    name: "",
    enabled: true,
    severity: kind === "uptime" || kind === "ssl_expiry" ? "critical" : "warning",
    params: emptyParams(kind),
    createdAtMs: 0,
    updatedAtMs: 0,
  };
}

export function RulesPanel() {
  const { servers, selectedServerId } = useServersContext();
  const [rules, setRules] = useState<IntelRule[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<IntelRule | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () => {
    listIntelRules()
      .then((next) => {
        setRules(next);
        setError(null);
      })
      .catch((err) => setError(String(err)));
  };

  useEffect(() => {
    load();
  }, []);

  const run = async () => {
    if (!selectedServerId) {
      toast.error("Select a server first");
      return;
    }
    setBusy(true);
    try {
      const out = await evaluateIntel(selectedServerId);
      const extra = out.errors.length ? ` ${out.errors[0]}` : "";
      toast.success(`Fired ${out.fired}, cleared ${out.cleared}.${extra}`);
      load();
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <AlertTemplates onChanged={load} />
      <Card>
        <CardHeader className="flex flex-row items-start justify-between gap-3">
          <div>
            <CardTitle className="text-base">Alert rules</CardTitle>
            <CardDescription>
              Thresholds on any metric, log pattern rate, error rate, p95, SSL expiry, and uptime.
              Each rule waits for its duration, then clears only after the value passes the
              hysteresis band. Built-in host rules stay on the Alerts page.
            </CardDescription>
          </div>
          <div className="flex shrink-0 gap-2">
            <Button type="button" variant="outline" size="sm" onClick={run} disabled={busy}>
              <Play aria-hidden /> Run checks
            </Button>
            <Button type="button" size="sm" onClick={() => setEditing(blank())}>
              New rule
            </Button>
          </div>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {error && (
            <Notice tone="critical" title="Could not load rules">
              {error}
            </Notice>
          )}
          {rules === null && !error && <Skeleton className="h-24 w-full" />}
          {rules && rules.length === 0 && (
            <EmptyState
              title="No intelligence rules yet"
              description="Add a threshold, a log pattern, an SSL expiry check, or an uptime check."
              action={
                <Button type="button" size="sm" onClick={() => setEditing(blank())}>
                  New rule
                </Button>
              }
            />
          )}
          {rules && rules.length > 0 && (
            <ul className="flex flex-col gap-2">
              {rules.map((rule) => (
                <li key={rule.id} className="flex flex-wrap items-center gap-2 rounded-lg border border-border px-3 py-2">
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-medium">
                      {rule.name}{" "}
                      {!rule.enabled && <Badge variant="outline">Disabled</Badge>}{" "}
                      {rule.firing && <Badge variant="destructive">Firing</Badge>}
                    </p>
                    <p className="font-mono text-xs text-muted-foreground">
                      {KIND_LABELS[rule.kind]} · {rule.severity} · for {rule.params.durationSecs}s · clear after{" "}
                      {rule.params.clearDurationSecs}s
                      {rule.params.metricKey ? ` · ${rule.params.metricKey}` : ""}
                      {rule.params.pattern ? ` · /${rule.params.pattern}/` : ""}
                      {rule.params.host ? (
                        <>
                          {" · "}
                          <Masked text={`${rule.params.host}:${rule.params.port}`} />
                        </>
                      ) : (
                        ""
                      )}
                    </p>
                  </div>
                  <SeverityBadge severity={rule.severity} />
                  <Button type="button" size="icon-sm" variant="ghost" aria-label={`Edit ${rule.name}`} onClick={() => setEditing(rule)}>
                    <Pencil aria-hidden />
                  </Button>
                  <Button
                    type="button"
                    size="icon-sm"
                    variant="ghost"
                    aria-label={`Delete ${rule.name}`}
                    onClick={async () => {
                      try {
                        await deleteIntelRule(rule.id);
                        load();
                      } catch (err) {
                        toast.error(String(err));
                      }
                    }}
                  >
                    <Trash2 aria-hidden />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
      {editing && (
        <RuleDialog
          rule={editing}
          servers={servers.map((server) => ({ id: server.id, name: server.name }))}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            load();
          }}
        />
      )}
    </div>
  );
}

function RuleDialog({
  rule,
  servers,
  onClose,
  onSaved,
}: {
  rule: IntelRule;
  servers: { id: string; name: string }[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const formId = useId();
  const [draft, setDraft] = useState(rule);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const low = ["ssl_expiry", "uptime", "app_down", "disk_forecast", "server_unreachable", "redis_down", "service_stopped"].includes(draft.kind);
  const needsMetric = draft.kind === "metric" || draft.kind === "error_rate" || draft.kind === "p95" || draft.kind === "disk_forecast";
  const invalid = !draft.name.trim() || !Number.isFinite(draft.params.threshold);

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{rule.id ? "Edit rule" : "New rule"}</DialogTitle>
          <DialogDescription>
            The condition must hold for the duration before it fires. It clears only after the value
            stays past the clear threshold.
          </DialogDescription>
        </DialogHeader>
        <form
          id={formId}
          className="grid gap-3"
          onSubmit={async (event) => {
            event.preventDefault();
            setSaving(true);
            setFormError(null);
            try {
              await upsertIntelRule({
                ...draft,
                name: draft.name.trim(),
                serverId: draft.serverId || null,
              });
              toast.success("Rule saved");
              onSaved();
            } catch (err) {
              setFormError(String(err));
            } finally {
              setSaving(false);
            }
          }}
        >
          {formError && (
            <Notice tone="critical" title="Could not save">
              {formError}
            </Notice>
          )}
          <div className="grid gap-1.5">
            <Label htmlFor={`${formId}-name`}>Name</Label>
            <Input id={`${formId}-name`} value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} required />
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-kind`}>Kind</Label>
              <SimpleSelect
                id={`${formId}-kind`}
                value={draft.kind}
                onValueChange={(next) => {
                  const kind = next as IntelKind;
                  setDraft({ ...draft, kind, params: { ...emptyParams(kind), ...draft.params, ...emptyParams(kind) } });
                }}
                options={KINDS.map((kind) => ({ value: kind, label: KIND_LABELS[kind] }))}
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-severity`}>Severity</Label>
              <SimpleSelect
                id={`${formId}-severity`}
                value={draft.severity}
                onValueChange={(next) => setDraft({ ...draft, severity: next as Severity })}
                options={[
                  { value: "info", label: "Info" },
                  { value: "warning", label: "Warning" },
                  { value: "critical", label: "Critical" },
                ]}
              />
            </div>
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor={`${formId}-server`}>Server</Label>
            <SimpleSelect
              id={`${formId}-server`}
              value={draft.serverId ?? "__all__"}
              onValueChange={(next) => setDraft({ ...draft, serverId: next === "__all__" ? null : next })}
              options={[
                { value: "__all__", label: "All servers" },
                ...servers.map((server) => ({ value: server.id, label: server.name })),
              ]}
            />
          </div>
          {needsMetric && (
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-metric`}>Metric key</Label>
              <Input
                id={`${formId}-metric`}
                list={`${formId}-metrics`}
                className="font-mono"
                value={draft.params.metricKey}
                onChange={(e) => setDraft({ ...draft, params: { ...draft.params, metricKey: e.target.value } })}
              />
              <datalist id={`${formId}-metrics`}>
                {METRIC_KEYS.map((key) => (
                  <option key={key} value={key} />
                ))}
              </datalist>
            </div>
          )}
          {draft.kind === "restart_loop" && (
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-loop-window`}>Window (seconds)</Label>
              <Input
                id={`${formId}-loop-window`}
                type="number"
                min={1}
                className="font-mono"
                value={draft.params.windowSecs}
                onChange={(e) => setDraft({ ...draft, params: { ...draft.params, windowSecs: Number(e.target.value) } })}
              />
            </div>
          )}
          {draft.kind === "log_pattern" && (
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="grid gap-1.5">
                <Label htmlFor={`${formId}-pattern`}>Pattern</Label>
                <Input
                  id={`${formId}-pattern`}
                  className="font-mono"
                  value={draft.params.pattern}
                  onChange={(e) => setDraft({ ...draft, params: { ...draft.params, pattern: e.target.value } })}
                  placeholder="ERROR|panic"
                />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor={`${formId}-window`}>Window (seconds)</Label>
                <Input
                  id={`${formId}-window`}
                  type="number"
                  min={1}
                  className="font-mono"
                  value={draft.params.windowSecs}
                  onChange={(e) => setDraft({ ...draft, params: { ...draft.params, windowSecs: Number(e.target.value) } })}
                />
              </div>
            </div>
          )}
          {(draft.kind === "ssl_expiry" || draft.kind === "uptime") && (
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="grid gap-1.5">
                <Label htmlFor={`${formId}-host`}>Host</Label>
                <SecretInput
                  id={`${formId}-host`}
                  kind="host"
                  value={draft.params.host}
                  onChange={(e) => setDraft({ ...draft, params: { ...draft.params, host: e.target.value } })}
                  autoComplete="off"
                />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor={`${formId}-port`}>Port</Label>
                <SecretInput
                  id={`${formId}-port`}
                  kind="port"
                  type="number"
                  min={1}
                  max={65535}
                  className="font-mono"
                  value={String(draft.params.port ?? "")}
                  onChange={(e) => setDraft({ ...draft, params: { ...draft.params, port: Number(e.target.value) } })}
                />
              </div>
            </div>
          )}
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-threshold`}>{low ? "Fire at or below" : "Fire at or above"}</Label>
              <Input
                id={`${formId}-threshold`}
                type="number"
                step="any"
                className="font-mono"
                value={Number.isNaN(draft.params.threshold) ? "" : draft.params.threshold}
                onChange={(e) =>
                  setDraft({ ...draft, params: { ...draft.params, threshold: e.target.value === "" ? NaN : Number(e.target.value) } })
                }
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-clear`}>{low ? "Clear above" : "Clear below"}</Label>
              <Input
                id={`${formId}-clear`}
                type="number"
                step="any"
                className="font-mono"
                value={draft.params.clearThreshold ?? ""}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    params: {
                      ...draft.params,
                      clearThreshold: e.target.value === "" ? undefined : Number(e.target.value),
                    },
                  })
                }
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-for`}>Sustained for (seconds)</Label>
              <Input
                id={`${formId}-for`}
                type="number"
                min={0}
                className="font-mono"
                value={draft.params.durationSecs}
                onChange={(e) => setDraft({ ...draft, params: { ...draft.params, durationSecs: Number(e.target.value) } })}
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-cooldown`}>Cooldown (seconds)</Label>
              <Input
                id={`${formId}-cooldown`}
                type="number"
                min={0}
                className="font-mono"
                value={draft.params.cooldownSecs ?? 0}
                onChange={(e) => setDraft({ ...draft, params: { ...draft.params, cooldownSecs: Number(e.target.value) } })}
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-quiet-start`}>Quiet from (minutes after midnight)</Label>
              <Input
                id={`${formId}-quiet-start`}
                type="number"
                min={0}
                max={1439}
                className="font-mono"
                value={draft.params.quietStartMin ?? ""}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    params: { ...draft.params, quietStartMin: e.target.value === "" ? null : Number(e.target.value) },
                  })
                }
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-quiet-end`}>Quiet until (minutes after midnight)</Label>
              <Input
                id={`${formId}-quiet-end`}
                type="number"
                min={0}
                max={1439}
                className="font-mono"
                value={draft.params.quietEndMin ?? ""}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    params: { ...draft.params, quietEndMin: e.target.value === "" ? null : Number(e.target.value) },
                  })
                }
              />
            </div>
            <div className="grid gap-1.5 sm:col-span-2">
              <Label htmlFor={`${formId}-app`}>App id (blank means every app)</Label>
              <Input
                id={`${formId}-app`}
                className="font-mono"
                value={draft.params.appId ?? ""}
                onChange={(e) => setDraft({ ...draft, params: { ...draft.params, appId: e.target.value } })}
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-clear-for`}>Stay clear for (seconds)</Label>
              <Input
                id={`${formId}-clear-for`}
                type="number"
                min={0}
                className="font-mono"
                value={draft.params.clearDurationSecs}
                onChange={(e) =>
                  setDraft({ ...draft, params: { ...draft.params, clearDurationSecs: Number(e.target.value) } })
                }
              />
            </div>
          </div>
          <label className="flex w-fit items-center gap-2 text-sm">
            <input
              type="checkbox"
              className="size-4 accent-primary"
              checked={draft.enabled}
              onChange={(e) => setDraft({ ...draft, enabled: e.target.checked })}
            />
            Enabled
          </label>
        </form>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" form={formId} disabled={invalid || saving}>
            Save rule
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
