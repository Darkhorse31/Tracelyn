import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { listAlertRules, upsertAlertRule } from "@/lib/alerts-api";
import type { AlertRule, RuleKind } from "@/lib/alerts-types";
import { listIntelRules, upsertIntelRule } from "@/pages/intel/api";
import { emptyParams, type IntelKind, type IntelRule, type Severity } from "@/pages/intel/types";

type Template = {
  title: string;
  detail: string;
  defaultOn: boolean;
  alertKind?: RuleKind;
  intelKind?: IntelKind;
  severity?: Severity;
};

const TEMPLATES: Template[] = [
  { title: "App crashed or exited", detail: "PM2 stopped or errored, Docker exited or OOM, systemd failed", defaultOn: true, intelKind: "app_crash", severity: "critical" },
  { title: "App restart loop", detail: "Five or more restarts inside 10 minutes", defaultOn: true, intelKind: "restart_loop", severity: "critical" },
  { title: "App is down", detail: "HTTP health check fails, or the app is no longer running", defaultOn: true, intelKind: "app_down", severity: "critical" },
  { title: "Disk almost full", detail: "Disk usage stays at or above 85%", defaultOn: true, alertKind: "disk_high" },
  { title: "Server unreachable", detail: "SSH stays disconnected", defaultOn: true, alertKind: "server_unreachable" },
  { title: "High CPU", detail: "CPU at or above 90% for 5 minutes", defaultOn: false, alertKind: "cpu_high" },
  { title: "High memory", detail: "Memory at or above 90% for 5 minutes", defaultOn: false, alertKind: "memory_high" },
  { title: "High load", detail: "Load average at or above 4", defaultOn: false, intelKind: "metric", severity: "warning" },
  { title: "High error rate in logs", detail: "Matching log lines per minute", defaultOn: false, intelKind: "log_pattern", severity: "warning" },
  { title: "New error type", detail: "A log signature that has not been seen before", defaultOn: false, intelKind: "new_error", severity: "warning" },
  { title: "Slow responses", detail: "Endpoint p95 over the threshold", defaultOn: false, alertKind: "endpoint_p95" },
  { title: "Disk filling up", detail: "Forecast says the disk fills within 7 days", defaultOn: false, intelKind: "disk_forecast", severity: "warning" },
  { title: "SSL certificate expiring", detail: "Certificate has 14 days or fewer", defaultOn: false, intelKind: "ssl_expiry", severity: "critical" },
  { title: "Redis down", detail: "Redis stops answering", defaultOn: false, intelKind: "redis_down", severity: "critical" },
  { title: "Redis memory high", detail: "Redis memory near its max", defaultOn: false, alertKind: "redis_memory_near_max" },
  { title: "Service stopped", detail: "nginx, apache, mysql, postgres, or redis is down", defaultOn: false, intelKind: "service_stopped", severity: "critical" },
  { title: "New SSH login", detail: "A login line that was not seen before", defaultOn: false, intelKind: "ssh_login", severity: "info" },
  { title: "Failed SSH login burst", detail: "Failed logins from auth.log cross the threshold", defaultOn: false, intelKind: "ssh_fail_burst", severity: "warning" },
  { title: "Deploy detected", detail: "An app comes back with a new version", defaultOn: false, intelKind: "deploy", severity: "info" },
];

function intelDraft(template: Template): IntelRule {
  const kind = template.intelKind ?? "metric";
  const params = emptyParams(kind);
  if (template.title === "High load") {
    params.metricKey = "load.1";
    params.threshold = 4;
    params.durationSecs = 300;
  }
  if (kind === "log_pattern") {
    params.pattern = "ERROR|FATAL|panic";
    params.threshold = 5;
  }
  return {
    id: "",
    serverId: null,
    kind,
    name: template.title,
    enabled: true,
    severity: template.severity ?? "warning",
    params,
    createdAtMs: 0,
    updatedAtMs: 0,
  };
}

export function AlertTemplates({ onChanged }: { onChanged?: () => void }) {
  const [intel, setIntel] = useState<IntelRule[]>([]);
  const [alerts, setAlerts] = useState<AlertRule[]>([]);
  const [busy, setBusy] = useState<string | null>(null);

  const load = () => {
    void listIntelRules().then(setIntel).catch((err) => toast.error(String(err)));
    void listAlertRules().then(setAlerts).catch((err) => toast.error(String(err)));
  };

  useEffect(() => {
    load();
  }, []);

  const enabled = (template: Template) => {
    if (template.alertKind) return alerts.some((rule) => rule.kind === template.alertKind && rule.enabled);
    return intel.some((rule) => rule.name === template.title && rule.enabled);
  };

  const toggle = async (template: Template) => {
    setBusy(template.title);
    try {
      const on = enabled(template);
      if (template.alertKind) {
        const rule = alerts.find((item) => item.kind === template.alertKind);
        if (!rule) {
          toast.error("That built-in rule is not on this machine yet");
          return;
        }
        await upsertAlertRule({ ...rule, enabled: !on, updatedAtMs: Date.now() });
      } else {
        const existing = intel.find((rule) => rule.name === template.title);
        if (existing && on) {
          await upsertIntelRule({ ...existing, enabled: false });
        } else if (existing) {
          await upsertIntelRule({ ...existing, enabled: true });
        } else {
          await upsertIntelRule(intelDraft(template));
        }
      }
      load();
      onChanged?.();
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Alert me when…</CardTitle>
        <CardDescription>
          Turn a template on with one click. Crash, restart loop, app down, disk, and server unreachable start enabled.
          Use the advanced editor below for thresholds, duration, severity, cooldown, quiet hours, and one app or server.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {intel.length === 0 && alerts.length === 0 ? (
          <p className="text-sm text-muted-foreground">Loading templates…</p>
        ) : (
          <ul className="grid gap-2 sm:grid-cols-2">
            {TEMPLATES.map((template) => {
              const on = enabled(template);
              return (
                <li key={template.title} className="flex items-start justify-between gap-3 rounded-lg border border-border px-3 py-2">
                  <div className="min-w-0">
                    <p className="text-sm font-medium">{template.title}</p>
                    <p className="text-xs text-muted-foreground">{template.detail}</p>
                  </div>
                  <Button
                    type="button"
                    size="sm"
                    variant={on ? "default" : "outline"}
                    aria-pressed={on}
                    disabled={busy === template.title}
                    onClick={() => void toggle(template)}
                  >
                    {on ? "On" : "Off"}
                  </Button>
                </li>
              );
            })}
          </ul>
        )}
        <p className="mt-3 text-xs text-muted-foreground">Unused templates stay off until you enable them.</p>
      </CardContent>
    </Card>
  );
}
