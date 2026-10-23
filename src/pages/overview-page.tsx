import { useState } from "react";
import { Plus } from "lucide-react";
import { toast } from "sonner";
import { ServerCard } from "@/components/servers/server-card";
import { ServerFormDialog } from "@/components/servers/server-form-dialog";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useServersContext } from "@/contexts/servers-context";
import type { ConnectionStatus, ServerRecord } from "@/lib/types";

function statusRank(status: ConnectionStatus | undefined): number {
  switch (status) {
    case "offline":
      return 0;
    case "degraded":
      return 1;
    case "connecting":
      return 2;
    case "disconnected":
      return 3;
    case "connected":
      return 4;
    default:
      return 5;
  }
}

function sortServers(
  servers: ServerRecord[],
  statusById: Map<string, { status: ConnectionStatus }>,
) {
  return [...servers].sort((a, b) => {
    const ra = statusRank(statusById.get(a.id)?.status);
    const rb = statusRank(statusById.get(b.id)?.status);
    if (ra !== rb) return ra - rb;
    return a.name.localeCompare(b.name);
  });
}

export function OverviewPage() {
  const { servers, statusById, loading, connect, disconnect } = useServersContext();
  const [addOpen, setAddOpen] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);

  const sorted = sortServers(servers, statusById);

  const handleConnect = async (id: string) => {
    setBusyId(id);
    try {
      await connect(id);
      toast.success("Connected");
    } catch (err) {
      toast.error("Connection failed", { description: String(err) });
    } finally {
      setBusyId(null);
    }
  };

  const handleDisconnect = async (id: string) => {
    setBusyId(id);
    try {
      await disconnect(id);
    } finally {
      setBusyId(null);
    }
  };

  if (loading) {
    return (
      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
        {Array.from({ length: 3 }).map((_, i) => (
          <Skeleton key={i} className="h-52 w-full rounded-xl" />
        ))}
      </div>
    );
  }

  if (servers.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-4 rounded-xl border border-dashed border-border bg-card/50 px-6 py-16 text-center">
        <h2 className="text-lg font-semibold">No servers configured</h2>
        <p className="max-w-md text-sm text-muted-foreground">
          Add a Linux VPS to verify SSH connectivity and see live status on this
          overview. Metrics and charts arrive in the next build steps.
        </p>
        <Button onClick={() => setAddOpen(true)}>
          <Plus data-icon="inline-start" />
          Add server
        </Button>
        <ServerFormDialog open={addOpen} onOpenChange={setAddOpen} />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between gap-2">
        <p className="text-sm text-muted-foreground">
          Unhealthy servers are sorted first. Connect to fetch OS details.
        </p>
        <Button variant="outline" size="sm" onClick={() => setAddOpen(true)}>
          <Plus data-icon="inline-start" />
          Add server
        </Button>
      </div>
      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
        {sorted.map((server) => (
          <ServerCard
            key={server.id}
            server={server}
            status={statusById.get(server.id)}
            busy={busyId === server.id}
            onConnect={() => handleConnect(server.id)}
            onDisconnect={() => handleDisconnect(server.id)}
          />
        ))}
      </div>
      <ServerFormDialog open={addOpen} onOpenChange={setAddOpen} />
    </div>
  );
}
