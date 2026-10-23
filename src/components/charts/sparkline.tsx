import { cn } from "cn";

interface SparklineProps {
  values: number[];
  className?: string;
  color?: string;
  max?: number;
}

export function Sparkline({
  values,
  className,
  color = "var(--chart-1)",
  max = 100,
}: SparklineProps) {
  const w = 120;
  const h = 32;
  const data = values.length > 0 ? values : [0];
  const min = 0;
  const top = max;
  const step = w / Math.max(data.length - 1, 1);

  const points = data
    .map((v, i) => {
      const x = i * step;
      const clamped = Math.min(top, Math.max(min, v));
      const y = h - (clamped / top) * h;
      return `${x},${y}`;
    })
    .join(" ");

  const area = `0,${h} ${points} ${w},${h}`;

  return (
    <svg
      viewBox={`0 0 ${w} ${h}`}
      className={cn("h-8 w-full", className)}
      preserveAspectRatio="none"
      aria-hidden
    >
      <polygon points={area} fill={color} fillOpacity={0.15} />
      <polyline
        points={points}
        fill="none"
        stroke={color}
        strokeWidth={1.5}
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}
