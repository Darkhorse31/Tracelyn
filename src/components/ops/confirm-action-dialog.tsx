import { useEffect, useId, useState } from "react";
import { toast } from "sonner";
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
import { SimpleSelect } from "@/components/ui/select";
import { opsPreview, opsRun } from "@/lib/ops-api";
import type { ActionOutcome, ActionRequest } from "@/lib/ops-types";

export function OpsConfirmDialog({
  open,
  onOpenChange,
  request,
  choices,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  request: ActionRequest | null;
  choices?: { id: string; label: string; request: ActionRequest }[];
}) {
  const id = useId();
  const [choiceId, setChoiceId] = useState(choices?.[0]?.id ?? "");
  const selected = choices?.find((choice) => choice.id === choiceId)?.request ?? request;
  const [useSudo, setUseSudo] = useState(false);
  const [pm2User, setPm2User] = useState("");
  const [pm2Home, setPm2Home] = useState("");
  const [typed, setTyped] = useState("");
  const [timeoutSecs, setTimeoutSecs] = useState(15);
  const [command, setCommand] = useState("");
  const [dangerous, setDangerous] = useState(false);
  const [confirmName, setConfirmName] = useState("");
  const [loading, setLoading] = useState(false);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<ActionOutcome | null>(null);

  useEffect(() => {
    if (!open) return;
    setChoiceId(choices?.[0]?.id ?? "");
    setTyped("");
    setOutcome(null);
    setError(null);
  }, [open, choices]);

  useEffect(() => {
    if (!open || !selected) return;
    setUseSudo(selected.useSudo);
    setPm2User(selected.pm2User ?? "");
    setPm2Home(selected.pm2Home ?? "");
    setTimeoutSecs(selected.healthTimeoutSecs ?? 15);
    setTyped("");
    setOutcome(null);
  }, [open, selected]);

  useEffect(() => {
    if (!open || !selected) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    const handle = window.setTimeout(() => {
      void opsPreview({
        ...selected,
        useSudo,
        pm2User: pm2User || null,
        pm2Home: pm2Home || null,
        healthTimeoutSecs: timeoutSecs,
      })
        .then((preview) => {
          if (cancelled) return;
          setCommand(preview.command);
          setDangerous(preview.dangerous);
          setConfirmName(preview.confirmName);
          setUseSudo((current) => (current === preview.useSudo ? current : preview.useSudo));
          setPm2User((current) =>
            current === "" && preview.pm2User ? preview.pm2User : current,
          );
          setPm2Home((current) =>
            current === "" && preview.pm2Home ? preview.pm2Home : current,
          );
        })
        .catch((err) => {
          if (!cancelled) setError(String(err));
        })
        .finally(() => {
          if (!cancelled) setLoading(false);
        });
    }, 200);
    return () => {
      cancelled = true;
      window.clearTimeout(handle);
    };
  }, [open, selected, useSudo, pm2User, pm2Home, timeoutSecs]);

  const nameOk = !dangerous || typed.trim() === confirmName;
  const verb = selected?.verb ?? "run";

  const submit = async () => {
    if (!selected) return;
    setRunning(true);
    setError(null);
    try {
      const result = await opsRun({
        ...selected,
        useSudo,
        pm2User: pm2User || null,
        pm2Home: pm2Home || null,
        confirmName: typed,
        healthTimeoutSecs: timeoutSecs,
      });
      setOutcome(result);
      if (result.exitCode !== 0) {
        toast.error("Command failed", { description: result.outputExcerpt || result.healthMessage });
      } else if (result.healthOk === false) {
        toast.warning(result.healthMessage, { description: result.outputExcerpt });
      } else {
        toast.success(result.healthMessage || "Command finished");
        onOpenChange(false);
      }
    } catch (err) {
      setError(String(err));
      toast.error("Command was not run", { description: String(err) });
    } finally {
      setRunning(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {selected?.kind === "process"
              ? `Send SIGTERM to ${selected.target}?`
              : `${verb[0]?.toUpperCase()}${verb.slice(1)} ${selected?.target ?? "target"}?`}
          </DialogTitle>
          <DialogDescription>
            This is the exact command that will run over SSH. Stop asks you to type the name.
            Afterwards Tracelyn checks that the service reaches the expected state.
          </DialogDescription>
        </DialogHeader>
        {choices && choices.length > 0 && (
          <div className="grid gap-2">
            <Label htmlFor={`${id}-choice`}>Application</Label>
            <SimpleSelect
              id={`${id}-choice`}
              value={choiceId}
              onValueChange={setChoiceId}
              options={choices.map((choice) => ({ value: choice.id, label: choice.label }))}
            />
          </div>
        )}
        {choices && choices.length === 0 && (
          <p className="text-sm text-muted-foreground">
            No PM2, systemd, Docker, nginx, or Apache application is on this server yet.
          </p>
        )}
        <div className="grid gap-2">
          <Label htmlFor={`${id}-command`}>Command</Label>
          <pre
            id={`${id}-command`}
            tabIndex={0}
            aria-label="Exact command"
            className="max-h-40 overflow-auto rounded-lg border border-border bg-muted p-3 font-mono text-xs whitespace-pre-wrap"
          >
            {loading ? "Building command…" : command || "—"}
          </pre>
        </div>
        {selected?.kind === "pm2" && (
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="grid gap-2">
              <Label htmlFor={`${id}-user`}>PM2 user</Label>
              <Input
                id={`${id}-user`}
                value={pm2User}
                onChange={(event) => setPm2User(event.target.value)}
                autoComplete="off"
                spellCheck={false}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor={`${id}-home`}>PM2_HOME</Label>
              <Input
                id={`${id}-home`}
                value={pm2Home}
                onChange={(event) => setPm2Home(event.target.value)}
                autoComplete="off"
                spellCheck={false}
                className="font-mono"
              />
            </div>
          </div>
        )}
        <label className="flex min-h-6 w-fit items-center gap-2 text-sm" htmlFor={`${id}-sudo`}>
          <input
            id={`${id}-sudo`}
            type="checkbox"
            className="size-4 accent-primary"
            checked={useSudo}
            onChange={(event) => setUseSudo(event.target.checked)}
          />
          Run with sudo
        </label>
        <div className="grid gap-2">
          <Label htmlFor={`${id}-wait`}>Wait for health (seconds)</Label>
          <Input
            id={`${id}-wait`}
            type="number"
            min={1}
            max={60}
            className="w-24 font-mono"
            value={timeoutSecs}
            onChange={(event) => setTimeoutSecs(Number(event.target.value))}
          />
        </div>
        {dangerous && (
          <div className="grid gap-2">
            <Label htmlFor={`${id}-name`}>Type {confirmName} to confirm stop</Label>
            <Input
              id={`${id}-name`}
              value={typed}
              onChange={(event) => setTyped(event.target.value)}
              autoComplete="off"
              spellCheck={false}
              aria-invalid={typed.length > 0 && !nameOk}
            />
          </div>
        )}
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        {outcome && (
          <div className="rounded-lg border border-border p-3 text-sm">
            <p>
              Exit {outcome.exitCode}. {outcome.healthMessage}
            </p>
            {outcome.outputExcerpt && (
              <pre className="mt-2 max-h-32 overflow-auto font-mono text-xs whitespace-pre-wrap">
                {outcome.outputExcerpt}
              </pre>
            )}
          </div>
        )}
        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            type="button"
            variant={dangerous ? "destructive" : "default"}
            disabled={!selected || !nameOk || loading || running || Boolean(error && !command)}
            onClick={() => void submit()}
          >
            {running ? "Running…" : "Run command"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
