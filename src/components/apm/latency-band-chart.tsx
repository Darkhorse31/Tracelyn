interface LatencyBandChartProps {
  points: { t: number; p50: number; p95: number; p99: number }[];
  height?: number;
}

export function LatencyBandChart({ points, height = 160 }: LatencyBandChartProps) {
  if (points.length < 2) return null;
  const w = 400;
  const max = Math.max(...points.flatMap((p) => [p.p99])) * 1.1;
  const x = (i: number) => (i / (points.length - 1)) * w;
  const y = (v: number) => height - (v / max) * (height - 8) - 4;

  const p50 = points.map((p, i) => `${x(i)},${y(p.p50)}`).join(" ");
  const p95 = points.map((p, i) => `${x(i)},${y(p.p95)}`).join(" ");
  const band95 = `${p50} ${points
    .map((p, i) => `${x(i)},${y(p.p95)}`)
    .reverse()
    .join(" ")}`;
  const band99 = `${p95} ${points
    .map((p, i) => `${x(i)},${y(p.p99)}`)
    .reverse()
    .join(" ")}`;

  return (
    <svg
      viewBox={`0 0 ${w} ${height}`}
      className="w-full"
      preserveAspectRatio="none"
      role="img"
      aria-label="Latency percentile bands"
    >
      <polygon points={band99} fill="var(--color-primary)" fillOpacity="0.12" />
      <polygon points={band95} fill="var(--color-primary)" fillOpacity="0.22" />
      <polyline
        points={p50}
        fill="none"
        stroke="var(--color-primary)"
        strokeWidth="1.5"
      />
    </svg>
  );
}
