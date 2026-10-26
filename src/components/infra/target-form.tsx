import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Notice } from "@/components/status/notice";
import { infraApi, type Target } from "./api";

export function TargetForm({
  serverId,
  engine,
  onSaved,
}: {
  serverId: string;
  engine: string;
  onSaved: () => void;
}) {
  const [target, setTarget] = useState<Target>({
    host: "",
    port: null,
    container: "",
    database: "",
  });
  const [hasCredentials, setHasCredentials] = useState(false);
  const [replace, setReplace] = useState(false);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    void infraApi
      .target(serverId, engine)
      .then((view) => {
        if (active) {
          setTarget(view.target);
          setHasCredentials(view.hasCredentials);
          setLoaded(true);
        }
      })
      .catch((e) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
    };
  }, [serverId, engine]);
  const save = async (clear = false) => {
    setBusy(true);
    setError(null);
    try {
      await infraApi.saveTarget(
        serverId,
        engine,
        target,
        clear
          ? { username: "", password: "" }
          : replace
            ? { username, password }
            : undefined,
      );
      setHasCredentials(
        clear ? false : replace ? !!(username || password) : hasCredentials,
      );
      setPassword("");
      setUsername("");
      setReplace(false);
      toast.success("Target saved. Collector refresh requested");
      onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const id = `target-${engine}`;
  return (
    <details className="rounded-lg border border-border p-4">
      <summary className="cursor-pointer text-sm font-medium">
        Connection and optional credentials
      </summary>
      <form
        className="mt-4 flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
        <p className="text-xs text-muted-foreground">
          {engine === "rabbitmq"
            ? "The management API is read from localhost on the server."
            : "The CLI runs on the server. A blank target uses localhost and auto-detects a matching Docker container if the CLI is absent."}{" "}
          {hasCredentials
            ? "Credentials are saved in Windows Credential Manager."
            : "No credentials saved. Existing local authentication is used."}
        </p>
        {error && (
          <Notice tone="critical" title="Could not save or read settings">
            {error} Check Windows Credential Manager access.
          </Notice>
        )}
        <div className="grid gap-4 sm:grid-cols-2">
          {engine !== "rabbitmq" && (
            <div className="space-y-2">
              <Label htmlFor={`${id}-host`}>Host on server</Label>
              <Input
                id={`${id}-host`}
                placeholder="localhost"
                value={target.host}
                onChange={(e) => setTarget({ ...target, host: e.target.value })}
              />
            </div>
          )}
          <div className="space-y-2">
            <Label htmlFor={`${id}-port`}>Port</Label>
            <Input
              id={`${id}-port`}
              type="number"
              min={1}
              max={65535}
              placeholder={String(
                { mysql: 3306, postgres: 5432, mongo: 27017, rabbitmq: 15672 }[
                  engine as "mysql" | "postgres" | "mongo" | "rabbitmq"
                ],
              )}
              value={target.port ?? ""}
              onChange={(e) =>
                setTarget({
                  ...target,
                  port: e.target.value ? Number(e.target.value) : null,
                })
              }
            />
          </div>
          {engine !== "rabbitmq" && (
            <>
              <div className="space-y-2">
                <Label htmlFor={`${id}-container`}>
                  Docker container (optional)
                </Label>
                <Input
                  id={`${id}-container`}
                  value={target.container}
                  onChange={(e) =>
                    setTarget({ ...target, container: e.target.value })
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor={`${id}-database`}>Database (optional)</Label>
                <Input
                  id={`${id}-database`}
                  value={target.database}
                  onChange={(e) =>
                    setTarget({ ...target, database: e.target.value })
                  }
                />
              </div>
            </>
          )}
        </div>
        <label className="flex min-h-6 items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={replace}
            onChange={(e) => setReplace(e.target.checked)}
          />
          {hasCredentials
            ? "Replace stored credentials"
            : "Save optional credentials"}
        </label>
        {replace && (
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-2">
              <Label htmlFor={`${id}-user`}>Monitoring username</Label>
              <Input
                id={`${id}-user`}
                autoComplete="off"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor={`${id}-password`}>Password</Label>
              <Input
                id={`${id}-password`}
                type="password"
                autoComplete="new-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </div>
          </div>
        )}
        <div className="flex gap-2">
          <Button type="submit" size="sm" disabled={busy || !loaded}>
            {busy ? "Saving…" : "Save connection"}
          </Button>
          {hasCredentials && (
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy || !loaded}
              onClick={() => void save(true)}
            >
              Clear saved credentials
            </Button>
          )}
        </div>
      </form>
    </details>
  );
}
