import { FleetHealth } from "@/components/alerts/fleet-health";
import { OverviewGauges } from "@/components/alerts/overview-gauges";
import { useState } from "react";
import { Plus } from "lucide-react";
import { toast } from "sonner";
import { reportHostKeyError } from "@/components/security/host-key-dialog";
import { ServerCard } from "@/components/servers/server-card";
import { InfraSummaryCard } from "@/components/infra/summary-card";
import { ServerFormDialog } from "@/components/servers/server-form-dialog";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useServersContext } from "@/contexts/servers-context";
import { useOverviewMetrics } from "@/hooks/use-overview-metrics";
import type { ConnectionStatus, ServerRecord } from "@/lib/types";
import { EmptyState, ErrorState } from "@/components/layout/enterprise";
import { PageHeader } from "@/components/layout/page-header";

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

function healthRank(
  status: ConnectionStatus | undefined,
  cpu?: number,
  mem?: number,
): number {
  const base = statusRank(status);
  if (status !== "connected") return base;
  const hot = (cpu ?? 0) > 85 || (mem ?? 0) > 85;
  return hot ? 3.5 : base;
}

function sortServers(
  servers: ServerRecord[],
  statusById: Map<string, { status: ConnectionStatus }>,
  metricsById: Map<string, { cpuPercent: number; memoryPercent: number }>,
) {
  return [...servers].sort((a, b) => {
    const ma = metricsById.get(a.id);
    const mb = metricsById.get(b.id);
    const ra = healthRank(
      statusById.get(a.id)?.status,
      ma?.cpuPercent,
      ma?.memoryPercent,
    );
    const rb = healthRank(
      statusById.get(b.id)?.status,
      mb?.cpuPercent,
      mb?.memoryPercent,
    );
    if (ra !== rb) return ra - rb;
    return a.name.localeCompare(b.name);
  });
}

export function OverviewPage() {
  const { servers, statusById, loading, error, refresh, connect, disconnect, selectedServerId } =
    useServersContext();
  const metricsById = useOverviewMetrics(true);
  const [addOpen, setAddOpen] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);

  const sorted = sortServers(servers, statusById, metricsById);

  const handleConnect = async (id: string) => {
    setBusyId(id);
    try {
      await connect(id);
      toast.success("Connected");
    } catch (err) {
      if (!reportHostKeyError(err)) {
        toast.error("We could not connect to this server", { description: "Check the server address and your connection details, then try again." });
      }
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
    if (error) return <ErrorState retry={() => { void refresh().catch(() => toast.error("We could not load your servers. Try again.")); }} />;
    return <><EmptyState title="Add your first server" description="Connect a server to see its health and get notified when something needs attention." action={<Button onClick={() => setAddOpen(true)}><Plus aria-hidden />Add server</Button>} /><ServerFormDialog open={addOpen} onOpenChange={setAddOpen} /></>;
  }

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title="Overview"
        description="See which servers need attention and what to do next."
        actions={
          <Button variant="outline" onClick={() => setAddOpen(true)}>
            <Plus data-icon="inline-start" />
            Add server
          </Button>
        }
      />
      <FleetHealth />
      {selectedServerId && <OverviewGauges key={selectedServerId} serverId={selectedServerId} connected={statusById.get(selectedServerId)?.status === "connected"} />}
      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
        {sorted.map((server) => (
          <ServerCard
            key={server.id}
            server={server}
            status={statusById.get(server.id)}
            metrics={metricsById.get(server.id)}
            busy={busyId === server.id}
            onConnect={() => handleConnect(server.id)}
            onDisconnect={() => handleDisconnect(server.id)}
          />
        ))}
      </div>
      {selectedServerId && <InfraSummaryCard serverId={selectedServerId} />}
      <ServerFormDialog open={addOpen} onOpenChange={setAddOpen} />
    </div>
  );
}
