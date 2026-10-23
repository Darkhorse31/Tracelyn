import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "cn";

interface MetricCardProps {
  title: string;
  value: string;
  subtitle?: string;
  loading?: boolean;
  className?: string;
}

export function MetricCard({
  title,
  value,
  subtitle,
  loading,
  className,
}: MetricCardProps) {
  return (
    <Card className={cn("min-h-[88px]", className)}>
      <CardHeader className="pb-1">
        <CardTitle className="text-xs font-medium text-muted-foreground">
          {title}
        </CardTitle>
      </CardHeader>
      <CardContent>
        {loading ? (
          <Skeleton className="h-8 w-24" />
        ) : (
          <>
            <p className="font-mono text-2xl font-semibold tabular-nums">{value}</p>
            {subtitle && (
              <p className="mt-1 text-xs text-muted-foreground">{subtitle}</p>
            )}
          </>
        )}
      </CardContent>
    </Card>
  );
}
