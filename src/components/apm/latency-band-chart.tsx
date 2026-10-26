import { type ReactNode, useMemo } from "react";
import {
  type ChartBand,
  type ChartSeries,
  UPlotChart,
} from "@/components/charts/uplot-chart";
import { CHART_DASHES } from "@/lib/chart-theme";

interface LatencyBandChartProps {
  title: string;
  /** `t` in Unix ms. */
  points: { t: number; p50: number; p95: number; p99: number }[];
  height?: number;
  className?: string;
  action?: ReactNode;
}

const BANDS: ChartBand[] = [
  { upper: 1, lower: 0, color: "vermillion" },
  { upper: 2, lower: 1, color: "purple" },
];

export const formatMs = (v: number) =>
  v >= 1000 ? `${(v / 1000).toFixed(2)} s` : `${v.toFixed(0)} ms`;

export function LatencyBandChart({
  title,
  points,
  height = 200,
  className,
  action,
}: LatencyBandChartProps) {
  const timestamps = useMemo(() => points.map((p) => p.t / 1000), [points]);
  const series = useMemo(
    (): ChartSeries[] => [
      { label: "p50", color: "blue", fill: false, values: points.map((p) => p.p50) },
      {
        label: "p95",
        color: "vermillion",
        fill: false,
        dash: CHART_DASHES[1],
        values: points.map((p) => p.p95),
      },
      {
        label: "p99",
        color: "purple",
        fill: false,
        dash: CHART_DASHES[2],
        values: points.map((p) => p.p99),
      },
    ],
    [points],
  );

  return (
    <UPlotChart
      title={title}
      timestamps={timestamps}
      series={series}
      bands={BANDS}
      formatValue={formatMs}
      height={height}
      className={className}
      action={action}
    />
  );
}
