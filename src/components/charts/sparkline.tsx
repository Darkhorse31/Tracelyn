import { useId } from "react";
import { cn } from "cn";
import { type ChartColor, chartColorVar } from "@/lib/chart-theme";

interface SparklineProps {
  values: number[];
  className?: string;
  color?: ChartColor;
  max?: number;
}

export function Sparkline({
  values,
  className,
  color = "blue",
  max = 100,
}: SparklineProps) {
  const gradientId = `spark-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;

  if (values.length < 2) {
    return (
      <div
        className={cn(
          "flex h-8 w-full items-center text-[11px] text-muted-foreground",
          className,
        )}
      >
        Waiting for data
      </div>
    );
  }

  const stroke = chartColorVar(color);
  const w = 120;
  const h = 32;
  const step = w / (values.length - 1);

  const points = values
    .map((v, i) => {
      const clamped = Math.min(max, Math.max(0, v));
      return `${i * step},${h - (clamped / max) * h}`;
    })
    .join(" ");

  return (
    <svg
      viewBox={`0 0 ${w} ${h}`}
      className={cn("h-8 w-full", className)}
      preserveAspectRatio="none"
      aria-hidden
    >
      <defs>
        <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={stroke} stopOpacity={0.25} />
          <stop offset="100%" stopColor={stroke} stopOpacity={0} />
        </linearGradient>
      </defs>
      <polygon points={`0,${h} ${points} ${w},${h}`} fill={`url(#${gradientId})`} />
      <polyline
        points={points}
        fill="none"
        stroke={stroke}
        strokeWidth={1.75}
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}
