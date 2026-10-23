import { useState } from "react";
import { AlertTriangle, History, Pencil, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { useAlerts } from "@/hooks/use-alerts";
import { useServersContext } from "@/contexts/servers-context";
import type { AlertRule } from "@/lib/alerts-types";
import { cn } from "@/lib/utils";

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
  onSave: (rule: AlertRule) => void;
  onCancel: () => void;
}) {
  const [draft, setDraft] = useState(rule);

  return (
    <div className="grid gap-4 rounded-lg border border-border bg-card/60 p-4 md:grid-cols-2">
      <div className="space-y-2">
        <Label htmlFor="rule-name">Name</Label>
        <Input
          id="rule-name"
          value={draft.name}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="rule-threshold">Threshold</Label>
        <Input
          id="rule-threshold"
          type="number"
          className="font-mono tabular-nums"
          value={draft.params.threshold}
          onChange={(e) =>
            setDraft({
              ...draft,
              params: { ...draft.params, threshold: Number(e.target.value) },
            })
          }
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="rule-clear">Clear below</Label>
        <Input
          id="rule-clear"
          type="number"
          className="font-mono tabular-nums"
          value={draft.params.clearThreshold ?? ""}
          onChange={(e) =>
            setDraft({
              ...draft,
              params: {
                ...draft.params,
                clearThreshold: e.target.value
                  ? Number(e.target.value)
                  : undefined,
              },
            })
          }
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="rule-duration">Duration (seconds)</Label>
        <Input
          id="rule-duration"
          type="number"
          className="font-mono tabular-nums"
          value={draft.params.durationSecs ?? 0}
          onChange={(e) =>
            setDraft({
              ...draft,
              params: {
                ...draft.params,
                durationSecs: Number(e.target.value),
              },
            })
          }
        />
      </div>
      <div className="flex items-center gap-2 md:col-span-2">
        <input
          id="rule-enabled"
          type="checkbox"
          checked={draft.enabled}
          onChange={(e) => setDraft({ ...draft, enabled: e.target.checked })}
          className="size-4 rounded border-border"
        />
        <Label htmlFor="rule-enabled">Enabled</Label>
      </div>
      <div className="flex gap-2 md:col-span-2">
        <Button size="sm" onClick={() => onSave({ ...draft, updatedAtMs: Date.now() })}>
          Save rule
        </Button>
        <Button size="sm" variant="outline" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </div>
  );
}

export function AlertsPage() {
  const { selectedServerId } = useServersContext();
  const { rules, history, loading, error, saveRule, removeRule } =
    useAlerts(selectedServerId ?? undefined);
  const [editingId, setEditingId] = useState<string | null>(null);

  return (
    <div className="grid gap-6 lg:grid-cols-2">
      <Card className="border-border/80">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <AlertTriangle className="size-4 text-status-warning" />
            Alert rules
          </CardTitle>
          <CardDescription>
            Default thresholds from the Tracelyn spec; edit per metric with hysteresis
            to reduce flapping.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          {loading && (
            <div className="space-y-2">
              <Skeleton className="h-16 w-full" />
              <Skeleton className="h-16 w-full" />
            </div>
          )}
          {error && (
            <p className="text-sm text-status-critical">{error}</p>
          )}
          {!loading &&
            rules.map((rule) => (
              <div key={rule.id} className="space-y-2">
                {editingId === rule.id ? (
                  <RuleEditor
                    rule={rule}
                    onSave={async (r) => {
                      await saveRule(r);
                      setEditingId(null);
                    }}
                    onCancel={() => setEditingId(null)}
                  />
                ) : (
                  <div
                    className="flex flex-col gap-2 rounded-lg border border-border bg-muted/20 p-4 sm:flex-row sm:items-center sm:justify-between"
                  >
                    <div className="min-w-0 space-y-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <p className="font-medium">{rule.name}</p>
                        <Badge variant={rule.enabled ? "default" : "secondary"}>
                          {rule.enabled ? "On" : "Off"}
                        </Badge>
                      </div>
                      <p className="font-mono text-xs tabular-nums text-muted-foreground">
                        {rule.kind} · fire ≥ {rule.params.threshold}
                        {rule.params.clearThreshold != null &&
                          ` · clear < ${rule.params.clearThreshold}`}
                        {rule.params.durationSecs
                          ? ` · ${rule.params.durationSecs}s`
                          : ""}
                      </p>
                    </div>
                    <div className="flex shrink-0 gap-1">
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label="Edit rule"
                        onClick={() => setEditingId(rule.id)}
                      >
                        <Pencil />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label="Delete rule"
                        onClick={() => void removeRule(rule.id)}
                      >
                        <Trash2 />
                      </Button>
                    </div>
                  </div>
                )}
              </div>
            ))}
        </CardContent>
      </Card>

      <Card className="border-border/80">
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <History className="size-4 text-status-info" />
            Alert history
          </CardTitle>
          <CardDescription>
            {selectedServerId
              ? "Filtered to the selected server."
              : "Recent events across all servers."}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {loading ? (
            <Skeleton className="h-48 w-full" />
          ) : history.length === 0 ? (
            <p className="text-sm text-muted-foreground">No alerts fired yet.</p>
          ) : (
            <ScrollArea className="h-[min(420px,50vh)] pr-3">
              <ul className="space-y-3">
                {history.map((evt) => (
                  <li
                    key={evt.id}
                    className="rounded-lg border border-border bg-card/40 p-3"
                  >
                    <div className="flex items-start justify-between gap-2">
                      <p className="text-sm font-medium">{evt.ruleName}</p>
                      <Badge
                        className={cn(
                          !evt.resolvedAtMs && "bg-status-critical text-white",
                        )}
                        variant={evt.resolvedAtMs ? "secondary" : "default"}
                      >
                        {evt.resolvedAtMs ? "Resolved" : "Active"}
                      </Badge>
                    </div>
                    <p className="mt-1 text-sm text-muted-foreground">{evt.message}</p>
                    <p className="mt-2 font-mono text-xs tabular-nums text-muted-foreground">
                      {formatTs(evt.firedAtMs)}
                      {evt.resolvedAtMs && ` → ${formatTs(evt.resolvedAtMs)}`}
                    </p>
                  </li>
                ))}
              </ul>
            </ScrollArea>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
