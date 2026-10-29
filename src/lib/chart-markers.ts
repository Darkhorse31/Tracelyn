export interface PlotMarker {
  id: string;
  tsMs: number;
  kind: string;
  label: string;
  detail: string;
}

export const MARKER_KIND_LABEL: Record<string, string> = {
  deploy: "Deploy",
  restart: "Restart",
  alert: "Alert",
  ops: "Ops action",
  cron: "Cron run",
  error: "Error",
  anomaly: "Anomaly",
  audit: "Audit",
};

export function markerKindLabel(kind: string): string {
  return MARKER_KIND_LABEL[kind] ?? kind;
}

/** Markers whose timestamp falls inside the plotted window, in time order. */
export function markersInView(markers: PlotMarker[], startSec: number, endSec: number): PlotMarker[] {
  return markers
    .filter((marker) => {
      const sec = marker.tsMs / 1000;
      return sec >= startSec && sec <= endSec;
    })
    .sort((a, b) => a.tsMs - b.tsMs);
}

/**
 * The marker closest to the cursor, when it is within 2% of the visible span.
 * Span is in seconds.
 */
export function markerNear(markers: PlotMarker[], cursorSec: number, spanSec: number): PlotMarker | null {
  if (markers.length === 0 || !(spanSec > 0)) return null;
  const slack = Math.max(spanSec * 0.02, 1);
  let best: PlotMarker | null = null;
  let bestDist = Infinity;
  for (const marker of markers) {
    const dist = Math.abs(marker.tsMs / 1000 - cursorSec);
    if (dist <= slack && dist < bestDist) {
      best = marker;
      bestDist = dist;
    }
  }
  return best;
}
