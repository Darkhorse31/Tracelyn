import { Badge } from "@/components/ui/badge";
import { CircleAlert, TriangleAlert, Info } from "lucide-react";

export function formatTs(ms: number) {
  return new Date(ms).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function SeverityBadge({ severity }: { severity: string }) {
  const label = severity === "critical" ? "Critical" : severity === "warning" ? "Warning" : "Info";
  const variant = severity === "critical" ? "destructive" : severity === "warning" ? "outline" : "secondary";
  const Icon = severity === "critical" ? CircleAlert : severity === "warning" ? TriangleAlert : Info;
  return <Badge variant={variant}><Icon className="size-3" aria-hidden />{label}</Badge>;
}

export function statusLabel(status: string) {
  if (status === "ack") return "Acknowledged";
  if (status === "resolved") return "Resolved";
  return "Open";
}

export const fieldClass =
  "h-8 w-full rounded-lg border border-input bg-transparent px-2.5 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring dark:bg-input/30";
