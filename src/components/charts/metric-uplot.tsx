import { AnomalyRibbon } from "./baseline-insights"
import { useMemo } from "react";
import { UPlotChart } from "@/components/charts/uplot-chart";
import type { ChartColor } from "@/lib/chart-theme";
import type { MetricPoint } from "@/lib/types";

export const UPLOT_SYNC_KEY = "tracelyn-server";

interface MetricUplotProps {
  baselineServerId?: string
  metricKey?: string
  title: string;
  /** Legend label; defaults to `title`. */
  label?: string;
  points: MetricPoint[];
  formatValue?: (v: number) => string;
  color?: ChartColor;
  fill?: boolean;
  className?: string;
  height?: number;
  onPointClick?: (timestampMs: number) => void;
}

function formatNumber(v: number): string {
  return v.toFixed(2);
}

export function MetricUplot({
  baselineServerId,
  metricKey,
  title,
  label = title,
  points,
  formatValue = formatNumber,
  color = "blue",
  fill = true,
  className,
  height = 180,
  onPointClick,
}: MetricUplotProps) {
  const timestamps = useMemo(() => points.map((p) => p.t / 1000), [points]);
  const series = useMemo(
    () => [{ label, color, fill, values: points.map((p) => p.v) }],
    [label, color, fill, points],
  );

  return (
    <div><UPlotChart
      title={title}
      timestamps={timestamps}
      series={series}
      formatValue={formatValue}
      height={height}
      syncKey={UPLOT_SYNC_KEY}
      className={className}
      onPointClick={onPointClick}
    />{baselineServerId && metricKey && <AnomalyRibbon serverId={baselineServerId} metric={metricKey} points={points} />}</div>
  );
}

export function formatBytesPerSec(bps: number): string {
  if (bps >= 1_000_000_000) return `${(bps / 1_000_000_000).toFixed(1)} GB/s`;
  if (bps >= 1_000_000) return `${(bps / 1_000_000).toFixed(1)} MB/s`;
  if (bps >= 1_000) return `${(bps / 1_000).toFixed(1)} KB/s`;
  return `${bps.toFixed(0)} B/s`;
}

export function formatPercent(v: number): string {
  return `${v.toFixed(1)}%`;
}
