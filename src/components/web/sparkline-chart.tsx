import { useId } from "react";
import { cn } from "cn";
import { type ChartColor, chartColorVar } from "@/lib/chart-theme";

interface Point {
  t: number;
  v: number;
}

interface SparklineChartProps {
  data: Point[];
  /** Names the series in the accessible summary. */
  label: string;
  formatValue?: (v: number) => string;
  height?: number;
  color?: ChartColor;
  className?: string;
}

export function SparklineChart({
  data,
  label,
  formatValue = (v) => v.toFixed(1),
  height = 48,
  color = "blue",
  className,
}: SparklineChartProps) {
  const gradientId = `spark-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;

  if (data.length < 2) {
    return (
      <div
        className={cn(
          "flex items-center justify-center text-sm text-muted-foreground",
          className,
        )}
      >
        Waiting for data
      </div>
    );
  }

  const stroke = chartColorVar(color);
  const w = 200;
  const min = Math.min(...data.map((d) => d.v));
  const max = Math.max(...data.map((d) => d.v));
  const span = max - min || 1;
  const pts = data
    .map((d, i) => {
      const x = (i / (data.length - 1)) * w;
      const y = height - ((d.v - min) / span) * (height - 4) - 2;
      return `${x},${y}`;
    })
    .join(" ");

  const area = `0,${height} ${pts} ${w},${height}`;

  return (
    <svg
      viewBox={`0 0 ${w} ${height}`}
      className={className}
      preserveAspectRatio="none"
      role="img"
      aria-label={`${label}: latest ${formatValue(data[data.length - 1].v)}, min ${formatValue(min)}, max ${formatValue(max)}`}
    >
      <defs>
        <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={stroke} stopOpacity="0.25" />
          <stop offset="100%" stopColor={stroke} stopOpacity="0" />
        </linearGradient>
      </defs>
      <polygon points={area} fill={`url(#${gradientId})`} />
      <polyline
        points={pts}
        fill="none"
        stroke={stroke}
        strokeWidth="1.75"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}
