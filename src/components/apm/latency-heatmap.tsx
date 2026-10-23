import { Fragment } from "react";
import type { LatencyHeatmapCell } from "@/lib/apm-types";

interface LatencyHeatmapProps {
  cells: LatencyHeatmapCell[];
  routes: string[];
}

function heatColor(ms: number) {
  const t = Math.min(1, ms / 800);
  const r = Math.round(30 + t * 200);
  const g = Math.round(120 - t * 80);
  const b = Math.round(200 - t * 160);
  return `rgb(${r},${g},${b})`;
}

export function LatencyHeatmap({ cells, routes }: LatencyHeatmapProps) {
  const buckets = 24;
  const byRoute = new Map<string, Map<number, LatencyHeatmapCell>>();
  for (const c of cells) {
    if (!byRoute.has(c.route)) byRoute.set(c.route, new Map());
    byRoute.get(c.route)!.set(c.bucketIndex, c);
  }

  return (
    <div className="overflow-x-auto">
      <div className="min-w-[640px]">
        <div
          className="grid gap-px"
          style={{
            gridTemplateColumns: `minmax(140px,1.2fr) repeat(${buckets}, minmax(12px, 1fr))`,
          }}
        >
          <div className="text-xs text-muted-foreground">Route</div>
          {Array.from({ length: buckets }, (_, i) => (
            <div key={i} className="text-center text-[10px] text-muted-foreground">
              {i}h
            </div>
          ))}
          {routes.map((route) => (
            <Fragment key={route}>
              <div
                className="truncate py-1 pr-2 font-mono text-xs text-foreground"
                title={route}
              >
                {route}
              </div>
              {Array.from({ length: buckets }, (_, b) => {
                const cell = byRoute.get(route)?.get(b);
                const ms = cell?.p95Ms ?? 0;
                return (
                  <div
                    key={`${route}-${b}`}
                    className="aspect-square min-h-3 rounded-sm"
                    style={{ backgroundColor: heatColor(ms) }}
                    title={cell ? `p95 ${ms.toFixed(0)} ms` : undefined}
                  />
                );
              })}
            </Fragment>
          ))}
        </div>
      </div>
    </div>
  );
}
