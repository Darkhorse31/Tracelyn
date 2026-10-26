import { useSyncExternalStore } from "react";
import type uPlot from "uplot";

/**
 * Okabe-Ito hues (see `--chart-*` in index.css). Charts that draw several series
 * together use `blue`, `vermillion`, `purple` in that order plus `CHART_DASHES`,
 * so series stay distinguishable under protan/deutan/tritan vision.
 */
export const CHART_COLORS = [
  "blue",
  "sky",
  "green",
  "orange",
  "vermillion",
  "purple",
] as const;

export type ChartColor = (typeof CHART_COLORS)[number];

/** Second, non-color cue for multi-series charts: solid, dashed, dotted. */
export const CHART_DASHES: readonly (number[] | undefined)[] = [
  undefined,
  [6, 4],
  [2, 3],
];

/** For SVG/DOM charts, which resolve CSS variables natively. */
export function chartColorVar(color: ChartColor): string {
  return `var(--chart-${color})`;
}

/** Tick labels use a monospace face: canvas cannot enable `tnum` on proportional fonts. */
export const CHART_FONT = '500 11px "JetBrains Mono", ui-monospace, monospace';

export const CHART_LINE_WIDTH = 1.75;

export interface ChartTheme {
  /** Opaque series strokes as `rgb()` strings. */
  colors: Record<ChartColor, string>;
  areaTop: Record<ChartColor, string>;
  areaBottom: Record<ChartColor, string>;
  /** Flat translucent fill for percentile bands between two series. */
  band: Record<ChartColor, string>;
  axis: string;
  grid: string;
  surface: string;
}

type Rgba = [number, number, number, number];

let probe: CanvasRenderingContext2D | null = null;

function getProbe(): CanvasRenderingContext2D {
  if (!probe) {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 1;
    probe = canvas.getContext("2d", { willReadFrequently: true })!;
  }
  return probe;
}

/** Canvas rejects `var(...)`, so resolve any CSS color to concrete channels. */
function toRgba(cssColor: string): Rgba {
  const ctx = getProbe();
  ctx.fillStyle = "#000000";
  ctx.fillStyle = cssColor;
  const serialized = String(ctx.fillStyle);

  const hex = /^#([0-9a-f]{6})$/i.exec(serialized);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
  }
  const rgb = /^rgba?\(([^)]+)\)$/i.exec(serialized);
  if (rgb) {
    const [r, g, b, a = 1] = rgb[1].split(",").map(Number);
    return [r, g, b, a];
  }

  // oklch()/lab() etc. stay in their own syntax when serialized; rasterize instead.
  ctx.clearRect(0, 0, 1, 1);
  ctx.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
  return [r, g, b, a / 255];
}

function rgba([r, g, b, a]: Rgba, alpha = a): string {
  return alpha >= 1
    ? `rgb(${r}, ${g}, ${b})`
    : `rgba(${r}, ${g}, ${b}, ${+alpha.toFixed(3)})`;
}

function resolveChartTheme(): ChartTheme {
  const style = getComputedStyle(document.documentElement);
  const read = (name: string) => toRgba(style.getPropertyValue(name).trim());

  const colors = {} as Record<ChartColor, string>;
  const areaTop = {} as Record<ChartColor, string>;
  const areaBottom = {} as Record<ChartColor, string>;
  const band = {} as Record<ChartColor, string>;
  for (const color of CHART_COLORS) {
    const c = read(`--chart-${color}`);
    colors[color] = rgba(c, 1);
    areaTop[color] = rgba(c, 0.25);
    areaBottom[color] = rgba(c, 0);
    band[color] = rgba(c, 0.14);
  }

  return {
    colors,
    areaTop,
    areaBottom,
    band,
    axis: rgba(read("--muted-foreground")),
    grid: rgba(read("--chart-grid")),
    surface: rgba(read("--card"), 1),
  };
}

let current: ChartTheme | null = null;
const listeners = new Set<() => void>();
let watching = false;

export function getChartTheme(): ChartTheme {
  current ??= resolveChartTheme();
  return current;
}

function refresh() {
  current = resolveChartTheme();
  for (const listener of listeners) listener();
}

function startWatching() {
  if (watching) return;
  watching = true;

  // ThemeProvider flips `class` on <html>.
  new MutationObserver(refresh).observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["class", "style", "data-theme"],
  });
  // Covers "System" mode; wait a frame so any class update lands first.
  window
    .matchMedia("(prefers-color-scheme: dark)")
    .addEventListener("change", () => requestAnimationFrame(refresh));
  // Axis sizes are measured with the tick font, so re-measure once it loads.
  void document.fonts?.load(CHART_FONT).then(refresh, () => {});
}

export function subscribeChartTheme(listener: () => void): () => void {
  listeners.add(listener);
  startWatching();
  return () => {
    listeners.delete(listener);
  };
}

export function useChartTheme(): ChartTheme {
  return useSyncExternalStore(subscribeChartTheme, getChartTheme, getChartTheme);
}

// ---------------------------------------------------------------------------
// uPlot option builders. Colors are functions so `u.redraw()` picks up a new
// theme without recreating the plot.

export function themedSeries(
  label: string,
  color: ChartColor,
  { fill = true, dash }: { fill?: boolean; dash?: number[] } = {},
): uPlot.Series {
  return {
    label,
    width: CHART_LINE_WIDTH,
    dash,
    stroke: () => getChartTheme().colors[color],
    fill: fill
      ? (u) => {
          const theme = getChartTheme();
          const { top, height } = u.bbox;
          const gradient = u.ctx.createLinearGradient(0, top, 0, top + height);
          gradient.addColorStop(0, theme.areaTop[color]);
          gradient.addColorStop(1, theme.areaBottom[color]);
          return gradient;
        }
      : undefined,
    points: { show: false },
  };
}

function measureAxis(
  u: uPlot,
  values: string[] | null,
  axisIdx: number,
  cycleNum: number,
): number {
  const axis = u.axes[axisIdx];
  if (cycleNum > 1) return (axis as { _size?: number })._size ?? 40;
  let size = (axis.ticks?.show ? (axis.ticks.size ?? 0) : 0) + (axis.gap ?? 0);
  const longest = (values ?? []).reduce(
    (acc, v) => (v.length > acc.length ? v : acc),
    "",
  );
  if (longest) {
    u.ctx.font = (axis.font as unknown as [string])[0];
    size += u.ctx.measureText(longest).width / devicePixelRatio;
  }
  return Math.ceil(size);
}

export function themedAxes(formatY: (v: number) => string): uPlot.Axis[] {
  const base: uPlot.Axis = {
    stroke: () => getChartTheme().axis,
    font: CHART_FONT,
    grid: { show: true, stroke: () => getChartTheme().grid, width: 1 },
    ticks: { show: false },
    gap: 6,
  };
  return [
    base,
    {
      ...base,
      size: measureAxis,
      values: (_u, splits) => splits.map((v) => formatY(v)),
    },
  ];
}

export function themedCursor(
  colors: ChartColor[],
  syncKey?: string,
): uPlot.Cursor {
  return {
    sync: syncKey ? { key: syncKey } : undefined,
    drag: { x: false, y: false },
    points: {
      size: 8,
      width: 2,
      fill: () => getChartTheme().surface,
      stroke: (_u, seriesIdx) =>
        getChartTheme().colors[colors[seriesIdx - 1] ?? "blue"],
    },
  };
}
