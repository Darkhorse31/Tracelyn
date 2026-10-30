import type { ReactNode } from "react";
import { CircleCheck, CircleHelp, TriangleAlert, CircleAlert } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";

export { EmptyState } from "@/components/status/notice";
export { Skeleton } from "@/components/ui/skeleton";
export { PageHeader } from "@/components/layout/page-header";

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return <Card><CardHeader><CardTitle><h2>{title}</h2></CardTitle></CardHeader><CardContent className="space-y-4">{children}</CardContent></Card>;
}
export function StatCard({ label, value, description }: { label: string; value: ReactNode; description?: string }) {
  return <Card><CardContent className="space-y-2"><p className="text-sm text-muted-foreground">{label}</p><p className="text-2xl font-semibold tabular-nums">{value}</p>{description && <p className="text-sm text-muted-foreground">{description}</p>}</CardContent></Card>;
}
export function StatusBadge({ status }: { status: "healthy" | "warning" | "critical" | "unknown" }) {
  const styles = { healthy: [CircleCheck, "text-status-ok", "Healthy"], warning: [TriangleAlert, "text-status-warning", "Needs attention"], critical: [CircleAlert, "text-status-critical", "Critical"], unknown: [CircleHelp, "text-muted-foreground", "Not measured"] } as const;
  const [Icon, color, label] = styles[status];
  return <span className={`inline-flex items-center gap-2 text-sm font-medium ${color}`}><Icon className="size-4" aria-hidden />{label}</span>;
}
export function ErrorState({ retry }: { retry: () => void }) {
  return <Notice tone="critical" title="We could not load this information" action={<Button variant="outline" onClick={retry}>Try again</Button>}>Check your connection and try again.</Notice>;
}
export function DataTable({ caption, headings, children }: { caption: string; headings: string[]; children: ReactNode }) {
  return <div className="overflow-x-auto"><table className="w-full text-left text-sm"><caption className="sr-only">{caption}</caption><thead><tr>{headings.map(h => <th key={h} scope="col" className="border-b p-4 font-medium text-muted-foreground">{h}</th>)}</tr></thead><tbody>{children}</tbody></table></div>;
}
