interface Point {
  t: number;
  v: number;
}

interface SparklineChartProps {
  data: Point[];
  height?: number;
  stroke?: string;
  className?: string;
}

export function SparklineChart({
  data,
  height = 48,
  stroke = "var(--color-primary)",
  className,
}: SparklineChartProps) {
  if (data.length < 2) return null;
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
      aria-label="Request rate sparkline"
    >
      <defs>
        <linearGradient id="spark-fill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={stroke} stopOpacity="0.35" />
          <stop offset="100%" stopColor={stroke} stopOpacity="0" />
        </linearGradient>
      </defs>
      <polygon points={area} fill="url(#spark-fill)" />
      <polyline
        points={pts}
        fill="none"
        stroke={stroke}
        strokeWidth="1.5"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}
