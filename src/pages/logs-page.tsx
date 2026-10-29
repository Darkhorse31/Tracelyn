import { ScrollText } from "lucide-react";
import { LogExplorer } from "@/components/logs/log-explorer";
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
    <div className="flex h-[calc(100svh-7rem)] min-h-0 flex-col gap-2">
      <div className="shrink-0">
        <h2 className="text-lg font-semibold tracking-tight">Logs</h2>
        <p className="text-sm text-muted-foreground">
          {server.name}
          {connected ? "" : " is not connected. Lines appear after you connect."}
        </p>
      </div>
      <LogExplorer serverId={selectedServerId} title="All application logs" />
    </div>
  );
}
