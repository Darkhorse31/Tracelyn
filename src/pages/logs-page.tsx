import { ScrollText } from "lucide-react";
import { LogExplorer } from "@/components/logs/log-explorer";
import { PageHeader } from "@/components/layout/page-header";
import { EmptyState } from "@/components/status/notice";
import { useServersContext } from "@/contexts/servers-context";

export function LogsPage() {
  const { servers, selectedServerId, statusById } = useServersContext();
  const server = servers.find((s) => s.id === selectedServerId);
  const connected =
    selectedServerId &&
    ["connected", "degraded"].includes(statusById.get(selectedServerId)?.status ?? "");

  if (!server || !selectedServerId) {
    return (
      <EmptyState
        icon={ScrollText}
        title="No server selected"
        description="Select a server to tail application logs."
      />
    );
  }

  return (
    <div className="flex h-[calc(100svh-7rem)] min-h-0 flex-col gap-4">
      <PageHeader
        title="Logs"
        description={`${server.name}${connected ? "" : " is not connected. Lines appear after you connect."}`}
      />
      <LogExplorer serverId={selectedServerId} title="All application logs" />
    </div>
  );
}
