import { Bug } from "lucide-react";
import { useSearchParams } from "react-router-dom";
import { ErrorIssues } from "@/components/insight/error-issues";
import { PageHeader } from "@/components/layout/page-header";
import { EmptyState } from "@/components/status/notice";
import { useServersContext } from "@/contexts/servers-context";

export function ErrorsPage() {
  const { servers, selectedServerId } = useServersContext();
  const [params] = useSearchParams();
  const server = servers.find((item) => item.id === selectedServerId);
  if (!server || !selectedServerId) {
    return <EmptyState icon={Bug} title="No server selected" description="Select a server to see grouped errors." />;
  }
  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title="Errors"
        description={`${server.name}. Groups come from OpenTelemetry exceptions and error stacks in application logs.`}
      />
      <ErrorIssues serverId={selectedServerId} traceId={params.get("trace") ?? undefined} />
    </div>
  );
}
