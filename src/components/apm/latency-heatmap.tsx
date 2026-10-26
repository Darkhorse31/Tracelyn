import type { LatencyHeatmapCell } from "@/lib/apm-types";
import { formatMs } from "@/components/apm/latency-band-chart";

/** Matches `HEATMAP_MINUTES` in the Rust aggregator; index 0 is the oldest minute. */
export const HEATMAP_BUCKETS = 30;

/** Upper bounds (ms) of each `--heat-N` step; the last step is open-ended. */
const STEPS = [50, 100, 200, 400, 800] as const;

function stepFor(ms: number): number {
  const i = STEPS.findIndex((limit) => ms < limit);
  return i === -1 ? STEPS.length : i;
}

function stepLabel(i: number): string {
  if (i === 0) return `< ${STEPS[0]} ms`;
  if (i === STEPS.length) return `≥ ${STEPS[STEPS.length - 1]} ms`;
  return `${STEPS[i - 1]}–${STEPS[i]} ms`;
}

function minutesAgo(bucket: number): string {
  const ago = HEATMAP_BUCKETS - 1 - bucket;
  return ago === 0 ? "now" : `−${ago}m`;
}

interface LatencyHeatmapProps {
  cells: LatencyHeatmapCell[];
  routes: string[];
}

export function LatencyHeatmap({ cells, routes }: LatencyHeatmapProps) {
  if (routes.length === 0) {
    return (
      <div className="flex h-32 items-center justify-center rounded-lg border border-dashed border-border text-sm text-muted-foreground">
        No endpoint latency recorded yet.
      </div>
    );
  }

  const byRoute = new Map<string, Map<number, LatencyHeatmapCell>>();
  for (const c of cells) {
    if (!byRoute.has(c.route)) byRoute.set(c.route, new Map());
    byRoute.get(c.route)!.set(c.bucketIndex, c);
  }
  const buckets = Array.from({ length: HEATMAP_BUCKETS }, (_, i) => i);

  return (
    <div className="flex flex-col gap-4">
      <div className="overflow-x-auto" tabIndex={0} role="region" aria-label="Latency heatmap, scrollable">
        <table className="w-full min-w-[720px] border-separate border-spacing-0.5 text-xs">
          <caption className="sr-only">
            p95 latency per route for each of the last {HEATMAP_BUCKETS} minutes
          </caption>
          <thead>
            <tr>
              <th scope="col" className="w-48 pb-1 text-left font-medium text-muted-foreground">
                Route
              </th>
              {buckets.map((b) => (
                <th
                  key={b}
                  scope="col"
                  className="pb-1 text-center font-mono text-[11px] font-normal text-muted-foreground"
                >
                  <span className={b % 5 === 4 ? undefined : "sr-only"}>{minutesAgo(b)}</span>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {routes.map((route) => (
              <tr key={route}>
                <th
                  scope="row"
                  className="max-w-48 truncate pr-2 text-left font-mono font-normal text-foreground"
                  title={route}
                >
                  {route}
                </th>
                {buckets.map((b) => {
                  const cell = byRoute.get(route)?.get(b);
                  return (
                    <td
                      key={b}
                      className="h-6 min-w-4 rounded-sm"
                      style={{
                        backgroundColor: cell
                          ? `var(--heat-${stepFor(cell.p95Ms)})`
                          : "var(--muted)",
                      }}
                      title={
                        cell
                          ? `${route} · ${minutesAgo(b)} · p95 ${formatMs(cell.p95Ms)}`
                          : undefined
                      }
                    >
                      <span className="sr-only">
                        {cell ? `p95 ${formatMs(cell.p95Ms)}` : "no data"}
                      </span>
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <ul className="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-muted-foreground" aria-label="Heatmap legend">
        {Array.from({ length: STEPS.length + 1 }, (_, i) => (
          <li key={i} className="flex items-center gap-2">
            <span
              className="size-3 rounded-sm"
              style={{ backgroundColor: `var(--heat-${i})` }}
              aria-hidden
            />
            <span className="font-mono">{stepLabel(i)}</span>
          </li>
        ))}
        <li className="flex items-center gap-2">
          <span className="size-3 rounded-sm bg-muted ring-1 ring-border" aria-hidden />
          No requests
        </li>
      </ul>
    </div>
  );
}
