import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { DerivedRedisMetrics } from "@/lib/redis-types";
import { cn } from "@/lib/utils";

function formatBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(2)} GB`;
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function StatCard({
  title,
  value,
  sub,
  tone,
}: {
  title: string;
  value: string;
  sub?: string;
  tone?: "ok" | "warning" | "critical" | "info";
}) {
  return (
    <Card className="min-h-[104px]">
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium text-muted-foreground">
          {title}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-1">
        <p
          className={cn(
            "font-mono text-2xl font-medium tabular-nums tracking-tight",
            tone === "ok" && "text-status-ok",
            tone === "warning" && "text-status-warning",
            tone === "critical" && "text-status-critical",
            tone === "info" && "text-status-info",
          )}
        >
          {value}
        </p>
        {sub && <p className="text-xs text-muted-foreground">{sub}</p>}
      </CardContent>
    </Card>
  );
}

export function RedisStatCards({ derived }: { derived: DerivedRedisMetrics }) {
  const memRatio = derived.memoryUsedRatio;
  const memTone =
    memRatio != null && memRatio >= 0.9
      ? "critical"
      : memRatio != null && memRatio >= 0.75
        ? "warning"
        : undefined;

  const hitTone =
    derived.hitRatePercent < 80
      ? "warning"
      : derived.hitRatePercent >= 95
        ? "ok"
        : undefined;

  return (
    <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
      <StatCard
        title="Ops / sec"
        value={derived.opsPerSec.toLocaleString(undefined, {
          maximumFractionDigits: 0,
        })}
        sub="instantaneous"
      />
      <StatCard
        title="Hit rate"
        value={`${derived.hitRatePercent.toFixed(1)}%`}
        sub="keyspace hits / (hits + misses)"
        tone={hitTone}
      />
      <StatCard
        title="Memory"
        value={formatBytes(derived.memoryUsedBytes)}
        sub={
          derived.maxmemoryBytes > 0
            ? `${((memRatio ?? 0) * 100).toFixed(0)}% of max · peak ${formatBytes(derived.memoryPeakBytes)}`
            : `peak ${formatBytes(derived.memoryPeakBytes)}`
        }
        tone={memTone}
      />
      <StatCard
        title="Clients · role"
        value={String(derived.connectedClients)}
        sub={`replication: ${derived.role}`}
        tone="info"
      />
      <StatCard
        title="Total keys"
        value={derived.totalKeys.toLocaleString()}
        sub="across databases"
      />
      <StatCard
        title="Evictions"
        value={derived.evictedKeys.toLocaleString()}
        sub="since start"
        tone={derived.evictedKeys > 0 ? "warning" : undefined}
      />
      <StatCard
        title="Expirations"
        value={derived.expiredKeys.toLocaleString()}
        sub="keys expired"
      />
    </div>
  );
}
