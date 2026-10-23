import { Badge } from "@/components/ui/badge";
import type { ConnectionStatus } from "@/lib/types";
import { cn } from "cn";

const LABELS: Record<ConnectionStatus, string> = {
  disconnected: "Disconnected",
  connecting: "Connecting",
  connected: "Online",
  degraded: "Degraded",
  offline: "Offline",
};

const STYLES: Record<ConnectionStatus, string> = {
  disconnected: "bg-muted text-muted-foreground",
  connecting: "bg-status-info/15 text-status-info",
  connected: "bg-status-ok/15 text-status-ok",
  degraded: "bg-status-warning/15 text-status-warning",
  offline: "bg-status-critical/15 text-status-critical",
};

export function ConnectionStatusBadge({
  status,
  className,
}: {
  status: ConnectionStatus;
  className?: string;
}) {
  return (
    <Badge
      variant="outline"
      className={cn("border-transparent font-medium", STYLES[status], className)}
    >
      {LABELS[status]}
    </Badge>
  );
}
