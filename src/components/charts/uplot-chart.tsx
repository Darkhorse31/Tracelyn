import { type ReactNode, useEffect, useId, useMemo, useRef, useState } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import {
  type ChartColor,
  getChartTheme,
  themedAxes,
  themedCursor,
  themedSeries,
  useChartTheme,
} from "@/lib/chart-theme";
import { cn } from "cn";

export interface ChartSeries {
  label: string;
  color: ChartColor;
  values: (number | null)[];
  fill?: boolean;
  dash?: number[];
}

/** Shades the area between two series (indices into `series`). */
export interface ChartBand {
  upper: number;
  lower: number;
  color: ChartColor;
}

export interface UPlotChartProps {
  title: string;
  /** Unix seconds. */
  timestamps: number[];
  series: ChartSeries[];
  formatValue: (v: number) => string;
  bands?: ChartBand[];
  height?: number;
  syncKey?: string;
  className?: string;
  onPointClick?: (timestampMs: number) => void;
  /** Rendered at the end of the title row, e.g. a series picker. */
  action?: ReactNode;
}

function latest(values: (number | null)[]): number | null {
  for (let i = values.length - 1; i >= 0; i--) {
    const v = values[i];
    if (v != null) return v;
  }
  return null;
}

function formatTime(seconds: number): string {
  return new Date(seconds * 1000).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function formatSpan(seconds: number): string {
  if (seconds >= 2 * 86_400) return `${Math.round(seconds / 86_400)} days`;
  if (seconds >= 2 * 3_600) return `${Math.round(seconds / 3_600)} hours`;
  if (seconds >= 120) return `${Math.round(seconds / 60)} minutes`;
  return `${Math.round(seconds)} seconds`;
}

/** Text alternative for the canvas, read by screen readers instead of the plot. */
function describe(
  timestamps: number[],
  series: ChartSeries[],
  format: (v: number) => string,
): string {
  if (timestamps.length < 2) return "No data yet.";
  const span = formatSpan(timestamps[timestamps.length - 1] - timestamps[0]);
  const parts = series.map((s) => {
    const values = s.values.filter((v): v is number => v != null);
    if (values.length === 0) return `${s.label}: no data`;
    const min = Math.min(...values);
    const max = Math.max(...values);
    return `${s.label}: latest ${format(values[values.length - 1])}, min ${format(min)}, max ${format(max)}`;
  });
  return `Last ${span}. ${parts.join(". ")}.`;
}

function LegendSwatch({ color, dash }: { color: string; dash?: number[] }) {
  return (
    <svg width="16" height="8" aria-hidden className="shrink-0">
      <line
        x1="0"
        y1="4"
        x2="16"
        y2="4"
        stroke={color}
        strokeWidth="2"
        strokeDasharray={dash?.join(" ")}
      />
    </svg>
  );
}

export function UPlotChart({
  title,
  timestamps,
  series,
  formatValue,
  bands,
  height = 180,
  syncKey,
  className,
  action,
  onPointClick,
}: UPlotChartProps) {
  const id = useId();
  const theme = useChartTheme();
  const plotElRef = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);
  const [hidden, setHidden] = useState<ReadonlySet<string>>(() => new Set());

  const formatRef = useRef(formatValue);
  formatRef.current = formatValue;
  const clickRef = useRef(onPointClick);
  clickRef.current = onPointClick;

  const data = useMemo<uPlot.AlignedData>(
    () => [timestamps, ...series.map((s) => s.values)],
    [timestamps, series],
  );
  const dataRef = useRef(data);
  dataRef.current = data;

  const seriesRef = useRef(series);
  seriesRef.current = series;
  const hiddenRef = useRef(hidden);
  hiddenRef.current = hidden;

  const hasData = timestamps.length >= 2;
  const toggleable = series.length > 1;
  const shapeKey = JSON.stringify([
    series.map((s) => [s.label, s.color, s.fill ?? true, s.dash ?? null]),
    bands ?? null,
  ]);

  useEffect(() => {
    const el = plotElRef.current;
    if (!el || !hasData) return;

    const shape = seriesRef.current;
    const plot = new uPlot(
      {
        width: el.clientWidth || 400,
        height,
        class: "tracelyn-uplot",
        legend: { show: false },
        scales: { x: { time: true }, y: { auto: true } },
        axes: themedAxes((v) => formatRef.current(v)),
        series: [
          {},
          ...shape.map((s) => ({
            ...themedSeries(s.label, s.color, { fill: s.fill, dash: s.dash }),
            show: !hiddenRef.current.has(s.label),
          })),
        ],
        bands: bands?.map((b) => ({
          series: [b.upper + 1, b.lower + 1] as [number, number],
          fill: () => getChartTheme().band[b.color],
        })),
        cursor: themedCursor(
          shape.map((s) => s.color),
          syncKey,
        ),
        hooks: {
          setCursor: [(u) => setHoverIdx(u.cursor.idx ?? null)],
        },
      },
      dataRef.current,
      el,
    );
    plotRef.current = plot;
    const onClick = () => {
      const idx = plot.cursor.idx;
      const ts = idx == null ? null : plot.data[0]?.[idx];
      if (ts != null) clickRef.current?.(ts * 1000);
    };
    plot.over.addEventListener("click", onClick);

    const ro = new ResizeObserver(() => {
      plot.setSize({ width: el.clientWidth, height });
    });
    ro.observe(el);

    return () => {
      plot.over.removeEventListener("click", onClick);
      ro.disconnect();
      plot.destroy();
      plotRef.current = null;
      setHoverIdx(null);
    };
  }, [shapeKey, height, syncKey, hasData]);

  useEffect(() => {
    plotRef.current?.setData(data);
  }, [data]);

  useEffect(() => {
    plotRef.current?.redraw(false, true);
  }, [theme]);

  const toggle = (label: string, index: number) => {
    const next = new Set(hidden);
    if (next.has(label)) next.delete(label);
    else next.add(label);
    setHidden(next);
    plotRef.current?.setSeries(index + 1, { show: !next.has(label) });
  };

  const hovering = hoverIdx != null && hoverIdx < timestamps.length;
  const summary = useMemo(
    () => describe(timestamps, series, formatValue),
    [timestamps, series, formatValue],
  );

  return (
    <figure
      aria-labelledby={`${id}-title`}
      aria-describedby={`${id}-summary`}
      className={cn(
        "flex h-full flex-col gap-2 rounded-xl bg-card p-4 text-card-foreground ring-1 ring-foreground/10",
        className,
      )}
    >
      <div className="flex min-h-5 items-center gap-4">
        <h3 id={`${id}-title`} className="text-sm font-medium">
          {title}
        </h3>
        <span className="ml-auto font-mono text-xs text-muted-foreground" aria-hidden>
          {hovering ? formatTime(timestamps[hoverIdx]) : null}
        </span>
        {action}
      </div>
      <ul className="flex min-h-5 flex-wrap items-center gap-x-4 gap-y-1 text-xs">
        {series.map((s, i) => {
          const v = hovering ? s.values[hoverIdx] : latest(s.values);
          const off = hidden.has(s.label);
          const content = (
            <>
              <LegendSwatch color={theme.colors[s.color]} dash={s.dash} />
              <span className="text-muted-foreground">{s.label}</span>
              <span className="font-mono font-medium">
                {v == null ? "—" : formatValue(v)}
              </span>
            </>
          );
          return (
            <li key={s.label}>
              {toggleable ? (
                <button
                  type="button"
                  aria-pressed={!off}
                  aria-label={`${s.label} series`}
                  onClick={() => toggle(s.label, i)}
                  className={cn(
                    "-mx-1 flex min-h-6 items-center gap-2 rounded-md px-1 transition-opacity duration-150 hover:bg-muted",
                    off && "opacity-50 [&>span]:line-through",
                  )}
                >
                  {content}
                </button>
              ) : (
                <span className="flex min-h-6 items-center gap-2">{content}</span>
              )}
            </li>
          );
        })}
      </ul>
      <p id={`${id}-summary`} className="sr-only">
        {summary}
      </p>
      {hasData ? (
        <div ref={plotElRef} className="w-full" style={{ height }} aria-hidden />
      ) : (
        <div
          className="flex w-full items-center justify-center rounded-lg border border-dashed border-border text-sm text-muted-foreground"
          style={{ height }}
        >
          Waiting for data
        </div>
      )}
    </figure>
  );
}
