import { useMemo } from "react";
import {
  type ChartSeries,
  UPlotChart,
} from "@/components/charts/uplot-chart";
import type { RedisTimeSeriesPoint } from "@/lib/redis-types";

const REDIS_SYNC_KEY = "tracelyn-redis";

const formatOps = (v: number) => `${Math.round(v).toLocaleString()} ops/s`;
const formatMb = (v: number) => `${v.toFixed(1)} MB`;
const formatPercent = (v: number) => `${v.toFixed(1)}%`;

export function RedisCharts({ series }: { series: RedisTimeSeriesPoint[] }) {
  const timestamps = useMemo(() => series.map((p) => p.t / 1000), [series]);

  const ops = useMemo(
    (): ChartSeries[] => [
      { label: "Ops/s", color: "blue", values: series.map((p) => p.opsPerSec) },
    ],
    [series],
  );

  const memory = useMemo(
    (): ChartSeries[] => [
      {
        label: "Used",
        color: "purple",
        values: series.map((p) => p.memoryUsedBytes / 1_048_576),
      },
    ],
    [series],
  );

  const hitRate = useMemo(
    (): ChartSeries[] => [
      {
        label: "Hit rate",
        color: "green",
        values: series.map((p) => p.hitRatePercent),
      },
    ],
    [series],
  );

  return (
    <div className="grid gap-6 lg:grid-cols-2">
      <UPlotChart
        title="Throughput"
        timestamps={timestamps}
        series={ops}
        formatValue={formatOps}
        syncKey={REDIS_SYNC_KEY}
      />
      <UPlotChart
        title="Memory used"
        timestamps={timestamps}
        series={memory}
        formatValue={formatMb}
        syncKey={REDIS_SYNC_KEY}
      />
      <UPlotChart
        title="Cache hit rate"
        timestamps={timestamps}
        series={hitRate}
        formatValue={formatPercent}
        height={200}
        syncKey={REDIS_SYNC_KEY}
        className="lg:col-span-2"
      />
    </div>
  );
}
