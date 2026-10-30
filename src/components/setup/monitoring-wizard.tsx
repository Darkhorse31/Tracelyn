import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
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
import { Masked } from "@/contexts/privacy-context";
import { listenWhileMounted } from "@/lib/events";
import {
  monitoringSetupRetry,
  monitoringSetupStart,
  monitoringSetupStatus,
  monitoringSetupUndo,
  type MonitoringEvent,
  type MonitoringSetup,
} from "@/lib/api";
import { cn } from "@/lib/utils";

const STATUS_LABEL: Record<string, string> = {
  waiting: "Waiting",
  running: "Running",
  done: "Done",
  skipped: "Skipped",
  failed: "Needs attention",
};

export function MonitoringWizard({ serverId }: { serverId: string }) {
  const [wizard, setWizard] = useState<MonitoringSetup | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [password, setPassword] = useState("");
  const [picked, setPicked] = useState<Record<string, boolean>>({});

  useEffect(() => {
    let alive = true;
    void monitoringSetupStatus(serverId)
      .then((next) => {
        if (alive) setWizard(next);
      })
      .catch((err) => {
        if (alive) setError(String(err));
      });
    return () => {
      alive = false;
    };
  }, [serverId]);

  useEffect(() => {
    return listenWhileMounted<MonitoringEvent>("monitoring-setup", (event) => {
      if (event.payload.serverId === serverId) setWizard(event.payload.wizard);
    });
  }, [serverId]);

  useEffect(() => {
    if (wizard?.pause !== "chooseApps") return;
    setPicked((current) => {
      if (Object.keys(current).length > 0) return current;
      const next: Record<string, boolean> = {};
      for (const app of wizard.detected.nodes) next[app.name] = true;
      return next;
    });
  }, [wizard]);

  const run = async (sudoPassword?: string, apps?: string[]) => {
    setBusy(true);
    setError(null);
    try {
      const next = await monitoringSetupStart(serverId, sudoPassword, apps);
      setWizard(next);
      setPassword("");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const nodes = wizard?.detected.nodes ?? [];
  const askPassword = wizard?.pause === "needPassword";

  return (
    <section className="flex flex-col gap-4 rounded-lg border border-border bg-card p-4" aria-label="Set up monitoring">
      <div className="flex flex-wrap items-center gap-2">
        <Button type="button" disabled={busy} onClick={() => void run()}>
          {busy ? "Working…" : "Set up monitoring"}
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            setError(null);
            void monitoringSetupUndo(serverId)
              .then(setWizard)
              .catch((err) => setError(String(err)))
              .finally(() => setBusy(false));
          }}
        >
          Undo setup
        </Button>
      </div>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          <Masked text={error} />
        </p>
      )}
      {wizard?.summary && (
        <p className="text-sm text-foreground" aria-live="polite">
          <Masked text={wizard.summary} />
        </p>
      )}
      {wizard && (
        <ol className="flex flex-col gap-2">
          {wizard.steps.map((step) => (
            <li key={step.id} className="flex flex-col gap-2 rounded-lg border border-border px-3 py-3">
              <div className="flex flex-wrap items-center gap-2">
                <span className={cn("size-2 shrink-0 rounded-full", dot(step.status))} aria-hidden />
                <span className="text-sm font-medium text-foreground">{step.title}</span>
                <span className="text-xs text-muted-foreground">{STATUS_LABEL[step.status] ?? step.status}</span>
              </div>
              {step.detail && <p className="text-sm text-muted-foreground"><Masked text={step.detail} /></p>}
              {step.status === "failed" && (
                <div className="flex flex-col gap-2">
                  <p className="text-sm text-foreground"><Masked text={step.reason} /></p>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      type="button"
                      size="sm"
                      disabled={busy}
                      onClick={() => {
                        setBusy(true);
                        setError(null);
                        void monitoringSetupRetry(serverId)
                          .then(setWizard)
                          .catch((err) => setError(String(err)))
                          .finally(() => setBusy(false));
                      }}
                    >
                      Retry this step
                    </Button>
                  </div>
                  {step.details && (
                    <details className="text-sm">
                      <summary className="cursor-pointer text-muted-foreground">Show details</summary>
                      <pre className="mt-2 max-h-40 overflow-auto rounded-md bg-muted p-3 font-mono text-xs text-foreground">
                        <Masked text={step.details} />
                      </pre>
                    </details>
                  )}
                </div>
              )}
              {step.script && (
                <details className="text-sm">
                  <summary className="cursor-pointer text-muted-foreground">Show what this will run</summary>
                  <pre className="mt-2 max-h-40 overflow-auto rounded-md bg-muted p-3 font-mono text-xs text-foreground">
                    <Masked text={step.script} />
                  </pre>
                </details>
              )}
            </li>
          ))}
        </ol>
      )}
      {wizard?.pause === "chooseApps" && (
        <fieldset className="flex flex-col gap-2">
          <legend className="text-sm font-medium text-foreground">Node apps that can send traces</legend>
          {nodes.map((app) => (
            <label key={app.name} className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={picked[app.name] ?? false}
                onChange={(event) => setPicked((current) => ({ ...current, [app.name]: event.target.checked }))}
              />
              <span>{app.name}</span>
              <span className="truncate font-mono text-xs text-muted-foreground"><Masked text={app.cwd} /></span>
            </label>
          ))}
          <Button
            type="button"
            className="w-fit"
            disabled={busy}
            onClick={() => void run(undefined, nodes.filter((app) => picked[app.name]).map((app) => app.name))}
          >
            Continue
          </Button>
        </fieldset>
      )}
      <Dialog open={askPassword} onOpenChange={() => {}}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Administrator password</DialogTitle>
            <DialogDescription>
              Used only for this session. It stays in memory and is not saved on this PC.
            </DialogDescription>
          </DialogHeader>
          <form
            className="flex flex-col gap-4"
            onSubmit={(event) => {
              event.preventDefault();
              if (!password) return;
              void run(password);
            }}
          >
            <div className="flex flex-col gap-2">
              <Label htmlFor={`sudo-${serverId}`}>Password</Label>
              <Input
                id={`sudo-${serverId}`}
                type="password"
                autoComplete="off"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
            </div>
            <DialogFooter>
              <Button type="submit" disabled={busy || !password}>
                Continue
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </section>
  );
}

function dot(status: string) {
  if (status === "done") return "bg-status-ok";
  if (status === "failed") return "bg-status-critical";
  if (status === "running") return "bg-status-warning";
  return "bg-muted-foreground";
}
