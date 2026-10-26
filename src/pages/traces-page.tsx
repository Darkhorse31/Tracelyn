import { Waypoints } from "lucide-react";
import { OtelSetup } from "@/components/insight/otel-setup";
import { TraceBrowser } from "@/components/traces/trace-browser";
import { EmptyState } from "@/components/status/notice";
import { useServersContext } from "@/contexts/servers-context";

export function TracesPage() {
  const { servers, selectedServerId } = useServersContext();
  const server = servers.find((item) => item.id === selectedServerId);
  if (!server || !selectedServerId) {
    return (
      <EmptyState
        icon={Waypoints}
        title="No server selected"
        description="Select a server to search traces from its OpenTelemetry files."
      />
    );
  }
  return (
    <div className="flex flex-col gap-6">
      <div>
        <h2 className="text-lg font-semibold tracking-tight">Traces</h2>
        <p className="text-sm text-muted-foreground">
          {server.name}. Spans come from OTLP JSON files. Nothing is installed on the server.
        </p>
      </div>
      <TraceBrowser serverId={selectedServerId} />
      <OtelSetup serverId={selectedServerId} appId="" />
    </div>
  );
}
