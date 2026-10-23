import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { useEffect, useRef } from "react";
import type { MetricPoint } from "@/lib/types";
import { cn } from "cn";

export const UPLOT_SYNC_KEY = "tracelyn-server";

function toSeries(points: MetricPoint[]): [number[], number[]] {
  const xs: number[] = [];
  const ys: number[] = [];
  for (const p of points) {
    xs.push(p.t / 1000);
    ys.push(p.v);
  }
  if (xs.length === 0) {
    xs.push(Date.now() / 1000);
    ys.push(0);
  }
  return [xs, ys];
}

interface MetricUplotProps {
  title: string;
  points: MetricPoint[];
  unit?: string;
  formatValue?: (v: number) => string;
  color?: string;
  fill?: boolean;
  className?: string;
  height?: number;
}

export function MetricUplot({
  title,
  points,
  unit = "",
  formatValue,
  color = "#3b82f6",
  fill = true,
  className,
  height = 180,
}: MetricUplotProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);

  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;

    const fmt =
      formatValue ??
      ((v: number) => `${v.toFixed(1)}${unit ? ` ${unit}` : ""}`);

    const opts: uPlot.Options = {
      width: el.clientWidth,
      height,
      title,
      cursor: { sync: { key: UPLOT_SYNC_KEY } },
      scales: {
        x: { time: true },
        y: { auto: true },
      },
      axes: [
        {
          stroke: "var(--muted-foreground)",
          grid: { show: true, stroke: "var(--border)", width: 1 },
          ticks: { show: false },
        },
        {
          stroke: "var(--muted-foreground)",
          grid: { show: true, stroke: "var(--border)", width: 1 },
          values: (_u, vals) => vals.map((v) => fmt(v as number)),
        },
      ],
      series: [
        {},
        {
          label: title,
          stroke: color,
          width: 2,
          fill: fill ? `${color}33` : undefined,
          points: { show: false },
        },
      ],
    };

    const [xs, ys] = toSeries(points);
    const plot = new uPlot(opts, [xs, ys], el);
    plotRef.current = plot;

    const ro = new ResizeObserver(() => {
      plot.setSize({ width: el.clientWidth, height });
    });
    ro.observe(el);

    return () => {
      ro.disconnect();
      plot.destroy();
      plotRef.current = null;
    };
  }, [title, formatValue, unit, color, fill, height]);

  useEffect(() => {
    if (!plotRef.current) return;
    const [xs, ys] = toSeries(points);
    plotRef.current.setData([xs, ys]);
  }, [points]);

  return (
    <div
      className={cn(
        "rounded-xl border border-border bg-card/60 p-3 transition-colors duration-200",
        className,
      )}
    >
      <div ref={rootRef} className="w-full font-tabular" />
    </div>
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
