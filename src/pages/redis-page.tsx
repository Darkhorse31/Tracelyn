import { useEffect, useState } from "react";
import { Database, KeyRound, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { RedisCharts } from "@/components/redis/redis-charts";
import { RedisKeyspaceTable } from "@/components/redis/redis-keyspace-table";
import { RedisSlowlogTable } from "@/components/redis/redis-slowlog-table";
import { RedisStatCards } from "@/components/redis/redis-stat-cards";
import { HowToFix } from "@/components/setup/how-to-fix";
import { EmptyState, Notice } from "@/components/status/notice";
import { Badge } from "@/components/ui/badge";
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
import { SecretInput } from "@/contexts/privacy-context";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { useRedisDashboard } from "@/hooks/use-redis-dashboard";
import { getRedisTarget, testRedisConnection, type RedisTargetConfig } from "@/lib/api";
import { SimpleSelect } from "@/components/ui/select";
import { useServersContext } from "@/contexts/servers-context";
import { useTimeRange } from "@/contexts/time-range-context";

/** The collector's first poll runs ~10 s after connect; allow two cycles before giving up. */
const DETECT_TIMEOUT_MS = 25_000;

export function RedisPage() {
  const { selectedServerId, servers, statusById } = useServersContext();
  const { isLive } = useTimeRange();
  const {
    snapshot,
    series,
    loading,
    error,
    authConfigured,
    refresh,
    saveRedisConnection,
  } = useRedisDashboard();
  const [authOpen, setAuthOpen] = useState(false);
  const [password, setPassword] = useState("");
  const [username, setUsername] = useState("");
  const [target, setTarget] = useState<RedisTargetConfig>({});
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [timedOut, setTimedOut] = useState(false);

  const server = servers.find((s) => s.id === selectedServerId);
  const connection = selectedServerId ? statusById.get(selectedServerId)?.status : undefined;
  const connected = connection === "connected" || connection === "degraded";
  const waitingForLive = isLive && connected && !snapshot.collectedAtMs;

  useEffect(() => {
    setTimedOut(false);
    if (!waitingForLive) return;
    const timer = setTimeout(() => setTimedOut(true), DETECT_TIMEOUT_MS);
    return () => clearTimeout(timer);
  }, [waitingForLive, selectedServerId]);

  const handleSaveAuth = async () => {
    setSaving(true);
    try {
      await saveRedisConnection({
        password: password || undefined,
        username: username || undefined,
        target,
      });
      toast.success("Redis connection saved to Credential Manager");
      setAuthOpen(false);
      setPassword("");
    } catch (err) {
      toast.error("Could not save password", { description: String(err) });
    } finally {
      setSaving(false);
    }
  };

  if (!server) {
    return (
      <EmptyState
        icon={Database}
        title="No server selected"
        description="Select a server in the sidebar to see Redis throughput, memory and slow queries."
      />
    );
  }

  if (isLive && !connected) {
    return (
      <EmptyState
        icon={Database}
        title={`${server.name} is not connected`}
        description="Connect the server from the Overview page. Redis metrics are collected over the same SSH session."
      />
    );
  }

  const authDialog = (
    <Dialog open={authOpen} onOpenChange={setAuthOpen}>
      <DialogContent>
        <form
          className="contents"
          onSubmit={(e) => {
            e.preventDefault();
            void handleSaveAuth();
          }}
        >
          <DialogHeader>
            <DialogTitle>Redis AUTH password</DialogTitle>
            <DialogDescription>
              Password and ACL username are stored in Windows Credential Manager for {server.name}.
              Port, socket, and Docker container name are saved in the server config.
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-3">
            <div className="grid gap-1">
              <Label htmlFor="redis-user">ACL username</Label>
              <SecretInput
                id="redis-user"
                kind="user"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                autoComplete="off"
                placeholder="default"
              />
            </div>
            <div className="grid gap-1">
              <Label htmlFor="redis-pass">Password</Label>
              <SecretInput
                id="redis-pass"
                kind="secret"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="off"
              />
            </div>
            <div className="grid gap-1">
              <Label htmlFor="redis-port">Port</Label>
              <SecretInput
                id="redis-port"
                kind="port"
                inputMode="numeric"
                value={target.port == null ? "" : String(target.port)}
                onChange={(e) =>
                  setTarget({ ...target, port: e.target.value ? Number(e.target.value) : null })
                }
                placeholder="6379"
              />
            </div>
            <div className="grid gap-1">
              <Label htmlFor="redis-sock">Unix socket</Label>
              <SecretInput
                id="redis-sock"
                kind="secret"
                value={target.socket ?? ""}
                onChange={(e) => setTarget({ ...target, socket: e.target.value })}
                placeholder="/var/run/redis/redis.sock"
              />
            </div>
            <div className="grid gap-1">
              <Label htmlFor="redis-docker">Docker container</Label>
              <Input
                id="redis-docker"
                value={target.dockerContainer ?? ""}
                onChange={(e) => setTarget({ ...target, dockerContainer: e.target.value })}
                placeholder="redis"
              />
            </div>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={testing || !selectedServerId}
              onClick={() => {
                if (!selectedServerId) return;
                setTesting(true);
                void testRedisConnection(selectedServerId, target)
                  .then((message) => toast.success(message))
                  .catch((err) => toast.error("Redis connection failed", { description: String(err) }))
                  .finally(() => setTesting(false));
              }}
            >
              {testing ? "Testing…" : "Test connection"}
            </Button>
            <Button type="button" variant="outline" onClick={() => setAuthOpen(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={saving}>
              Save
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );

  const phase = snapshot.collectorState?.phase;
  const phaseMessage = snapshot.collectorState?.message;
  const instances = snapshot.instances ?? [];

  const openAuth = () => {
    if (selectedServerId) {
      void getRedisTarget(selectedServerId).then(setTarget).catch(() => undefined);
    }
    setAuthOpen(true);
  };

  if ((phase === "not_installed" || (waitingForLive && timedOut && !phase)) && !snapshot.collectedAtMs) {
    return (
      <>
        <EmptyState
          icon={Database}
          title={phase === "not_installed" ? "Redis is not installed" : "Redis did not respond"}
          description={
            phaseMessage ||
            `No redis-cli or Redis container was found on ${server.name}. Set a port, socket, or Docker container if Redis is not on the default path.`
          }
          action={
            <Button variant="outline" onClick={openAuth}>
              <KeyRound data-icon="inline-start" />
              Redis connection
            </Button>
          }
        />
        {authDialog}
      </>
    );
  }

  const header = (
    <div className="flex flex-wrap items-center justify-between gap-4">
      <div className="min-w-0">
        <h2 className="truncate text-lg font-semibold tracking-tight">{server.name}</h2>
        <p className="text-sm text-muted-foreground">
          {snapshot.info.redisVersion ? `Redis ${snapshot.info.redisVersion}` : "Redis"}
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant="outline">
          <KeyRound aria-hidden />
          {authConfigured ? "AUTH configured" : "AUTH not set"}
        </Badge>
        {instances.length > 1 && (
          <label className="flex items-center gap-2 text-xs">
            Instance
            <SimpleSelect
              id="redis-instance"
              ariaLabel="Redis instance"
              value={target.label ?? instances[0]?.label ?? ""}
              onValueChange={(next) => {
                const inst = instances.find((row) => row.label === next);
                if (!inst || !selectedServerId) return;
                const saved = {
                  label: inst.label,
                  port: inst.port,
                  socket: inst.socket,
                  dockerContainer: inst.dockerContainer,
                };
                setTarget(saved);
                void saveRedisConnection({ target: saved });
              }}
              options={instances.map((inst) => ({ value: inst.label, label: inst.label }))}
            />
          </label>
        )}
        <Button variant="outline" size="sm" onClick={openAuth}>
          <KeyRound data-icon="inline-start" />
          Redis connection
        </Button>
        {!isLive && (
          <Button variant="outline" size="sm" onClick={() => refresh()}>
            <RefreshCw data-icon="inline-start" />
            Refresh
          </Button>
        )}
      </div>
    </div>
  );

  return (
    <div className="flex flex-col gap-6">
      {header}

      {error && (
        <Notice tone="critical" title="Could not load Redis metrics">
          {error}
        </Notice>
      )}
      {!isLive && (
        <Notice tone="info">
          Showing stored history for throughput, memory and hit rate. Keyspace and slow log
          are only available in Live mode.
        </Notice>
      )}
      {isLive && phase === "auth_required" && (
        <Notice
          tone="warning"
          title="Redis needs a password"
          action={
            <Button type="button" size="sm" variant="outline" onClick={openAuth}>
              Set password
            </Button>
          }
        >
          {phaseMessage || "Redis needs a password."}
          <HowToFix text="redis authentication required" />
        </Notice>
      )}
      {isLive && phase === "protected_mode" && (
        <Notice tone="warning" title="Redis protected mode">
          {phaseMessage || "Redis protected mode rejected the connection. It only accepts loopback clients until a password is set."}
        </Notice>
      )}
      {isLive && phase === "connection_refused" && (
        <Notice tone="critical" title="Redis connection refused">
          {phaseMessage || "Nothing accepted the connection. Check the port, unix socket, or Docker container name."}
        </Notice>
      )}
      {isLive && phase === "timed_out" && (
        <Notice tone="warning" title="Redis timed out">
          {phaseMessage || "The connection timed out. Tracelyn will retry."}
        </Notice>
      )}
      {isLive && phase === "loading" && (
        <Notice tone="info" title="Loading Redis">
          Waiting for the first INFO response.
        </Notice>
      )}

      {isLive && phase && phase !== "live" && phase !== "loading" ? null : loading && !snapshot.collectedAtMs && isLive ? (
        <>
          <div className="grid gap-6 sm:grid-cols-2 xl:grid-cols-4">
            {Array.from({ length: 8 }, (_, i) => (
              <Skeleton key={i} className="h-[104px] rounded-xl" />
            ))}
          </div>
          <div className="grid gap-6 lg:grid-cols-2">
            <Skeleton className="h-[256px] rounded-xl" />
            <Skeleton className="h-[256px] rounded-xl" />
          </div>
        </>
      ) : (
        <>
          {isLive && <RedisStatCards derived={snapshot.derived} />}
          <RedisCharts series={series} />
          {isLive && (
            <div className="grid gap-6 xl:grid-cols-2">
              <RedisKeyspaceTable keyspace={snapshot.keyspace} />
              <RedisSlowlogTable entries={snapshot.slowlog} />
            </div>
          )}
        </>
      )}

      {authDialog}
    </div>
  );
}
