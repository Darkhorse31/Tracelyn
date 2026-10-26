import {
  CircleCheck,
  CircleDashed,
  CircleMinus,
  CircleX,
  Loader2,
  TriangleAlert,
  type LucideIcon,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import type { ConnectionStatus } from "@/lib/types";
import { cn } from "cn";

export const CONNECTION_STATUS_LABELS: Record<ConnectionStatus, string> = {
  disconnected: "Disconnected",
  connecting: "Connecting",
  connected: "Online",
  degraded: "Degraded",
  offline: "Offline",
};

const ICONS: Record<ConnectionStatus, LucideIcon> = {
  disconnected: CircleMinus,
  connecting: Loader2,
  connected: CircleCheck,
  degraded: TriangleAlert,
  offline: CircleX,
};

const TEXT: Record<ConnectionStatus, string> = {
  disconnected: "text-muted-foreground",
  connecting: "text-status-info",
  connected: "text-status-ok",
  degraded: "text-status-warning",
  offline: "text-status-critical",
};

const BADGE: Record<ConnectionStatus, string> = {
  disconnected: "bg-muted",
  connecting: "bg-status-info/15",
  connected: "bg-status-ok/15",
  degraded: "bg-status-warning/15",
  offline: "bg-status-critical/15",
};

/** Shape differs per state so status never relies on color alone. */
export function ConnectionStatusIcon({
  status,
  className,
}: {
  status: ConnectionStatus | undefined;
  className?: string;
}) {
  const Icon = status ? ICONS[status] : CircleDashed;
  return (
    <Icon
      aria-hidden
      className={cn(
        "size-3.5 shrink-0",
        status ? TEXT[status] : "text-muted-foreground",
        status === "connecting" && "animate-spin",
        className,
      )}
    />
  );
}

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
      className={cn(
        "border-transparent font-medium",
        BADGE[status],
        TEXT[status],
        className,
      )}
    >
      <ConnectionStatusIcon status={status} className="size-3" />
      {CONNECTION_STATUS_LABELS[status]}
    </Badge>
  );
}
