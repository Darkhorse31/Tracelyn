import type { ReactNode } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "cn";

interface MetricCardProps {
  title: string;
  /** Plain value, or custom content such as a sparkline, laid out in a fixed 32px row. */
  value: ReactNode;
  subtitle?: string;
  loading?: boolean;
  className?: string;
}

export function MetricCard({ title, value, subtitle, loading, className }: MetricCardProps) {
  return (
    <Card className={cn("h-full", className)}>
      <CardHeader>
        <CardTitle className="text-xs font-medium text-muted-foreground">{title}</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-1">
        {loading ? (
          <Skeleton className="h-8 w-24" />
        ) : typeof value === "string" ? (
          <p className="h-8 truncate font-mono text-2xl leading-8 font-semibold">{value}</p>
        ) : (
          <div className="h-8">{value}</div>
        )}
        {subtitle && <p className="truncate text-xs text-muted-foreground">{subtitle}</p>}
      </CardContent>
    </Card>
  );
}
