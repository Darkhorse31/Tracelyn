import { AlertTriangle, CircleCheck, CircleX } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { DerivedRedisMetrics } from "@/lib/redis-types";
import { cn } from "@/lib/utils";

function formatBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(2)} GB`;
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

type Tone = "ok" | "warning" | "critical";

const TONE_ICON = { ok: CircleCheck, warning: AlertTriangle, critical: CircleX };
const TONE_LABEL = { ok: "Healthy", warning: "Warning", critical: "Critical" };

function StatCard({
  title,
  value,
  sub,
  tone,
}: {
  title: string;
  value: string;
  sub?: string;
  tone?: Tone;
}) {
  const Icon = tone ? TONE_ICON[tone] : null;
  return (
    <Card className="h-full">
      <CardHeader>
        <CardTitle className="text-xs font-medium text-muted-foreground">{title}</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-1">
        <p
          className={cn(
            "flex items-center gap-2 font-mono text-2xl font-medium tracking-tight",
            tone === "ok" && "text-status-ok",
            tone === "warning" && "text-status-warning",
            tone === "critical" && "text-status-critical",
          )}
        >
          <span className="truncate">{value}</span>
          {Icon && tone && (
            <>
              <Icon className="size-4 shrink-0" aria-hidden />
              <span className="sr-only">({TONE_LABEL[tone]})</span>
            </>
          )}
        </p>
        {sub && <p className="truncate text-xs text-muted-foreground">{sub}</p>}
      </CardContent>
    </Card>
  );
}

export function RedisStatCards({ derived }: { derived: DerivedRedisMetrics }) {
  const memRatio = derived.memoryUsedRatio;
  const memTone: Tone | undefined =
    memRatio != null && memRatio >= 0.9
      ? "critical"
      : memRatio != null && memRatio >= 0.75
        ? "warning"
        : undefined;

  const hitTone: Tone | undefined =
    derived.hitRatePercent < 80 ? "warning" : derived.hitRatePercent >= 95 ? "ok" : undefined;

  return (
    <div className="grid gap-6 sm:grid-cols-2 xl:grid-cols-4">
      <StatCard
        title="Ops / sec"
        value={derived.opsPerSec.toLocaleString(undefined, { maximumFractionDigits: 0 })}
        sub="instantaneous"
      />
      <StatCard
        title="Hit rate"
        value={`${derived.hitRatePercent.toFixed(1)}%`}
        sub="hits / (hits + misses)"
        tone={hitTone}
      />
      <StatCard
        title="Memory"
        value={formatBytes(derived.memoryUsedBytes)}
        sub={
          derived.maxmemoryBytes > 0
            ? `${((memRatio ?? 0) * 100).toFixed(0)}% of max · peak ${formatBytes(derived.memoryPeakBytes)}`
            : `peak ${formatBytes(derived.memoryPeakBytes)} · no maxmemory`
        }
        tone={memTone}
      />
      <StatCard
        title="Clients"
        value={derived.connectedClients.toLocaleString()}
        sub="connected"
      />
      <StatCard title="Role" value={derived.role} sub="replication" />
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
        sub="since start"
      />
    </div>
  );
}
