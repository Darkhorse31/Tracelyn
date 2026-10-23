import { useMemo } from "react";
import type uPlot from "uplot";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { UPlotChart } from "@/components/charts/uplot-chart";
import type { RedisTimeSeriesPoint } from "@/lib/redis-types";

const chartColors = {
  ops: "oklch(0.62 0.17 255)",
  memory: "oklch(0.68 0.14 180)",
  hit: "oklch(0.72 0.16 145)",
};

function baseChartOpts(title: string, unit: string) {
  return {
    title,
    class: "tracelyn-uplot",
    scales: {
      x: { time: true },
      y: { auto: true },
    },
    axes: [
      {
        stroke: "oklch(0.55 0.02 255)",
        grid: { show: true, stroke: "oklch(0.35 0.02 255 / 0.35)", width: 1 },
        values: (_: uPlot, splits: number[]) =>
          splits.map((v) => {
            const d = new Date(v * 1000);
            return d.toLocaleTimeString(undefined, {
              hour: "2-digit",
              minute: "2-digit",
            });
          }),
      },
      {
        stroke: "oklch(0.55 0.02 255)",
        grid: { show: true, stroke: "oklch(0.35 0.02 255 / 0.35)", width: 1 },
        values: (_: uPlot, splits: number[]) =>
          splits.map((v) => `${v.toLocaleString()} ${unit}`),
      },
    ],
    cursor: {
      drag: { x: false, y: false },
      points: { show: true },
    },
    legend: { show: false },
  };
}

export function RedisCharts({ series }: { series: RedisTimeSeriesPoint[] }) {
  const timestamps = useMemo(
    () => series.map((p) => p.t / 1000),
    [series],
  );

  const opsData = useMemo(
    (): uPlot.AlignedData => [timestamps, series.map((p) => p.opsPerSec)],
    [series, timestamps],
  );

  const memData = useMemo((): uPlot.AlignedData => {
    const mb = series.map((p) => p.memoryUsedBytes / 1_048_576);
    return [timestamps, mb];
  }, [series, timestamps]);

  const hitData = useMemo(
    (): uPlot.AlignedData => [timestamps, series.map((p) => p.hitRatePercent)],
    [series, timestamps],
  );

  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-medium">Throughput</CardTitle>
        </CardHeader>
        <CardContent className="px-2 pb-4">
          <UPlotChart
            data={opsData}
            options={{
              ...baseChartOpts("", "ops/s"),
              series: [
                {},
                {
                  label: "Ops/s",
                  stroke: chartColors.ops,
                  width: 2,
                  fill: `${chartColors.ops}33`,
                },
              ],
            }}
          />
        </CardContent>
      </Card>
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-medium">Memory used</CardTitle>
        </CardHeader>
        <CardContent className="px-2 pb-4">
          <UPlotChart
            data={memData}
            options={{
              ...baseChartOpts("", "MB"),
              series: [
                {},
                {
                  label: "Memory",
                  stroke: chartColors.memory,
                  width: 2,
                  fill: `${chartColors.memory}33`,
                },
              ],
            }}
          />
        </CardContent>
      </Card>
      <Card className="lg:col-span-2">
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-medium">Cache hit rate</CardTitle>
        </CardHeader>
        <CardContent className="px-2 pb-4">
          <UPlotChart
            data={hitData}
            height={200}
            options={{
              ...baseChartOpts("", "%"),
              series: [
                {},
                {
                  label: "Hit %",
                  stroke: chartColors.hit,
                  width: 2,
                  fill: `${chartColors.hit}33`,
                },
              ],
            }}
          />
        </CardContent>
      </Card>
    </div>
  );
}
