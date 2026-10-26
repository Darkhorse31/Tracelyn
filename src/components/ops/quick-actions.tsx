import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { OpsConfirmDialog } from "@/components/ops/confirm-action-dialog";
import { Button } from "@/components/ui/button";
import { useServersContext } from "@/contexts/servers-context";
import { useOpsMode } from "@/hooks/use-ops";
import { listApps } from "@/lib/api";
import type { TracelynApp } from "@/lib/apps-types";
import {
  defaultSudo,
  defaultVerbs,
  type ActionRequest,
  type OpsKind,
  type OpsVerb,
} from "@/lib/ops-types";

const LABELS: Record<OpsVerb, string> = {
  start: "Start",
  stop: "Stop",
  restart: "Restart",
  reload: "Reload",
};

function isOpsKind(kind: string): kind is OpsKind {
  return kind === "pm2" || kind === "systemd" || kind === "docker" || kind === "nginx" || kind === "apache";
}

export function requestFromApp(app: TracelynApp, verb: OpsVerb): ActionRequest | null {
  if (!isOpsKind(app.kind)) return null;
  const verbList = defaultVerbs(app.kind);
  const actual = verbList.includes(verb) ? verb : verbList[0];
  return {
    serverId: app.serverId,
    kind: app.kind,
    verb: actual,
    target: app.name,
    logPaths: app.logSources.map((source) => source.path).filter((path): path is string => Boolean(path)),
    useSudo: defaultSudo(app.kind),
    healthTimeoutSecs: 15,
  };
}

export function OpsQuickActions({
  serverId,
  kind,
  target,
  appId,
  logPaths = [],
  verbs,
  includeNav = true,
  showHint = true,
}: {
  serverId: string;
  kind: OpsKind;
  target: string;
  appId?: string;
  logPaths?: string[];
  verbs?: OpsVerb[];
  includeNav?: boolean;
  showHint?: boolean;
}) {
  const { enabled } = useOpsMode();
  const navigate = useNavigate();
  const { setSelectedServerId } = useServersContext();
  const [request, setRequest] = useState<ActionRequest | null>(null);
  const shown = (verbs ?? defaultVerbs(kind)).filter((verb) => defaultVerbs(kind).includes(verb));

  const go = (path: string) => {
    setSelectedServerId(serverId);
    navigate(path);
  };

  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-wrap gap-1">
        {shown.map((verb) => (
          <Button
            key={verb}
            type="button"
            size="xs"
            variant={verb === "stop" ? "destructive" : "outline"}
            disabled={!enabled}
            aria-label={`${LABELS[verb]} ${target}`}
            onClick={() =>
              setRequest({
                serverId,
                kind,
                verb,
                target,
                logPaths,
                useSudo: defaultSudo(kind),
                healthTimeoutSecs: 15,
              })
            }
          >
            {LABELS[verb]}
          </Button>
        ))}
        {includeNav && (
          <Button
            type="button"
            size="xs"
            variant="ghost"
            onClick={() => go(appId ? `/apps/${appId}?tab=Logs` : "/logs")}
          >
            Logs
          </Button>
        )}
        {includeNav && (
          <Button
            type="button"
            size="xs"
            variant="ghost"
            disabled={!enabled}
            onClick={() => go(`/ops/terminal?new=${Date.now()}`)}
          >
            Terminal
          </Button>
        )}
      </div>
      {showHint && !enabled && (
        <p className="text-xs text-muted-foreground">Operations mode is off, so these actions stay disabled.</p>
      )}
      <OpsConfirmDialog
        open={request != null}
        onOpenChange={(open) => {
          if (!open) setRequest(null);
        }}
        request={request}
      />
    </div>
  );
}

export function AlertQuickActions({ serverId }: { serverId: string }) {
  const { enabled } = useOpsMode();
  const navigate = useNavigate();
  const { setSelectedServerId } = useServersContext();
  const [open, setOpen] = useState(false);
  const [choices, setChoices] = useState<{ id: string; label: string; request: ActionRequest }[]>([]);
  const [loading, setLoading] = useState(false);

  const go = (path: string) => {
    setSelectedServerId(serverId);
    navigate(path);
  };

  const openRestart = async () => {
    setLoading(true);
    setOpen(true);
    try {
      const apps = await listApps(serverId);
      const next = apps.flatMap((app) => {
        const verb = app.kind === "nginx" || app.kind === "apache" ? "reload" : "restart";
        const request = requestFromApp(app, verb);
        if (!request) return [];
        return [{ id: app.id, label: `${app.name} (${app.kind})`, request }];
      });
      setChoices(next);
    } catch (err) {
      toast.error("Could not load applications", { description: String(err) });
      setChoices([]);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="mt-2 flex flex-wrap gap-1">
      <Button type="button" size="xs" variant="outline" disabled={!enabled || loading} onClick={() => void openRestart()}>
        Restart
      </Button>
      <Button type="button" size="xs" variant="ghost" onClick={() => go("/logs")}>
        Logs
      </Button>
      <Button
        type="button"
        size="xs"
        variant="ghost"
        disabled={!enabled}
        onClick={() => go(`/ops/terminal?new=${Date.now()}`)}
      >
        Terminal
      </Button>
      <OpsConfirmDialog
        open={open}
        onOpenChange={setOpen}
        request={choices[0]?.request ?? null}
        choices={choices}
      />
    </div>
  );
}
