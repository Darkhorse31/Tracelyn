import { useState } from "react";
import { KeyRound, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { RedisCharts } from "@/components/redis/redis-charts";
import { RedisKeyspaceTable } from "@/components/redis/redis-keyspace-table";
import { RedisSlowlogTable } from "@/components/redis/redis-slowlog-table";
import { RedisStatCards } from "@/components/redis/redis-stat-cards";
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
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { useRedisDashboard } from "@/hooks/use-redis-dashboard";
import { useServersContext } from "@/contexts/servers-context";

export function RedisPage() {
  const { selectedServerId, servers } = useServersContext();
  const {
    snapshot,
    series,
    loading,
    error,
    authConfigured,
    refresh,
    setRedisPassword,
  } = useRedisDashboard();
  const [authOpen, setAuthOpen] = useState(false);
  const [password, setPassword] = useState("");
  const [saving, setSaving] = useState(false);

  const serverName =
    servers.find((s) => s.id === selectedServerId)?.name ?? "No server selected";

  const handleSaveAuth = async () => {
    setSaving(true);
    try {
      await setRedisPassword(password);
      toast.success("Redis AUTH password saved to Credential Manager");
      setAuthOpen(false);
      setPassword("");
    } catch (err) {
      toast.error("Could not save password", { description: String(err) });
    } finally {
      setSaving(false);
    }
  };

  if (loading && !snapshot.collectedAtMs) {
    return (
      <div className="flex flex-col gap-4">
        <Skeleton className="h-8 w-64" />
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-28 rounded-xl" />
          ))}
        </div>
        <Skeleton className="h-64 rounded-xl" />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold tracking-tight">Redis</h2>
          <p className="text-sm text-muted-foreground">
            {serverName}
            {snapshot.info.redisVersion && (
              <> · v{snapshot.info.redisVersion}</>
            )}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={authConfigured ? "default" : "outline"}>
            {authConfigured ? "AUTH configured" : "AUTH not set"}
          </Badge>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setAuthOpen(true)}
            disabled={!selectedServerId}
          >
            <KeyRound className="size-4" />
            Redis password
          </Button>
          <Button variant="outline" size="sm" onClick={() => refresh()}>
            <RefreshCw className="size-4" />
            Refresh
          </Button>
        </div>
      </div>

      {error && (
        <div
          className="rounded-lg border border-status-critical/40 bg-status-critical/10 px-4 py-3 text-sm text-status-critical"
          role="alert"
        >
          {error}
        </div>
      )}

      {!selectedServerId && (
        <p className="text-sm text-muted-foreground">
          Select a server in the sidebar to store Redis AUTH credentials. Metrics below use
          parser demo data until live collection is wired.
        </p>
      )}

      <RedisStatCards derived={snapshot.derived} />
      <RedisCharts series={series} />
      <div className="grid gap-4 xl:grid-cols-2">
        <RedisKeyspaceTable keyspace={snapshot.keyspace} />
        <RedisSlowlogTable entries={snapshot.slowlog} />
      </div>

      <Dialog open={authOpen} onOpenChange={setAuthOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Redis AUTH password</DialogTitle>
            <DialogDescription>
              Stored in Windows Credential Manager for the selected server (same pattern as SSH
              secrets). Used when redis-cli runs over SSH.
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-2 py-2">
            <Label htmlFor="redis-pass">Password</Label>
            <Input
              id="redis-pass"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="off"
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setAuthOpen(false)}>
              Cancel
            </Button>
            <Button onClick={() => void handleSaveAuth()} disabled={saving || !password}>
              Save
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
