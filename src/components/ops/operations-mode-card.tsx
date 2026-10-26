import { useEffect, useId, useState } from "react";
import { toast } from "sonner";
import { Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useServersContext } from "@/contexts/servers-context";
import { useOpsMode } from "@/hooks/use-ops";
import { opsSetSudoPassword, opsSudoConfigured } from "@/lib/ops-api";

export function OperationsModeCard() {
  const id = useId();
  const { enabled, loading, error, update } = useOpsMode();
  const { servers, selectedServerId } = useServersContext();
  const server = servers.find((row) => row.id === selectedServerId) ?? servers[0];
  const [busy, setBusy] = useState(false);
  const [password, setPassword] = useState("");
  const [sudoSet, setSudoSet] = useState(false);
  const [sudoError, setSudoError] = useState<string | null>(null);

  useEffect(() => {
    if (!server) return;
    let cancelled = false;
    opsSudoConfigured(server.id)
      .then((status) => {
        if (!cancelled) setSudoSet(status.configured);
      })
      .catch((err) => {
        if (!cancelled) setSudoError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [server]);

  const saveSudo = async () => {
    if (!server) return;
    setBusy(true);
    try {
      await opsSetSudoPassword(server.id, password);
      setPassword("");
      setSudoSet(true);
      setSudoError(null);
      toast.success("Sudo password saved in Windows Credential Manager");
    } catch (err) {
      toast.error("Could not save sudo password", { description: String(err) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Operations</CardTitle>
        <CardDescription>
          Off by default. Restart, reload, and the SSH terminal stay disabled until you turn this
          on. The password for sudo is stored in Windows Credential Manager, per server.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {error && (
          <Notice tone="critical" title="Could not read Operations mode">
            {error}
          </Notice>
        )}
        <label className="flex min-h-6 w-fit cursor-pointer items-center gap-2 text-sm" htmlFor={id}>
          <input
            id={id}
            type="checkbox"
            className="size-4 accent-primary"
            checked={enabled}
            disabled={loading || busy}
            onChange={async (event) => {
              setBusy(true);
              try {
                await update(event.target.checked);
              } catch (err) {
                toast.error("Could not update Operations mode", { description: String(err) });
              } finally {
                setBusy(false);
              }
            }}
          />
          Operations mode
        </label>
        <p className="text-sm text-muted-foreground">
          {enabled
            ? "Actions are armed. Each one still asks you to confirm the exact command."
            : "The app is read-only. Monitoring, logs, and traces keep working."}
        </p>
        <form
          className="grid gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void saveSudo();
          }}
        >
          <Label htmlFor={`${id}-sudo`}>
            Sudo password{server ? ` for ${server.name}` : ""}
          </Label>
          <div className="flex flex-wrap gap-2">
            <Input
              id={`${id}-sudo`}
              type="password"
              autoComplete="off"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              placeholder={sudoSet ? "Saved. Enter a new password to replace it." : "Not saved"}
              className="max-w-sm"
              disabled={!server}
            />
            <Button type="submit" variant="outline" disabled={!server || !password || busy}>
              Save password
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">
            {sudoError
              ? sudoError
              : sudoSet
                ? "A sudo password is saved for this server. It is sent on stdin to sudo -S and is not written to the audit log."
                : "No sudo password saved. Commands that need sudo use sudo -n and fail if a password is required."}
          </p>
        </form>
      </CardContent>
    </Card>
  );
}

export function OpsModeBanner() {
  const { enabled, loading } = useOpsMode();
  if (loading || enabled) return null;
  return (
    <Notice tone="warning" title="Operations mode is off">
      The app is read-only. Turn on Operations mode in Settings before restarting a service or
      opening a shell.
    </Notice>
  );
}
