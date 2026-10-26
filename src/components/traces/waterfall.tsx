import { useEffect, useRef } from "react";
import { CHART_FONT, useChartTheme } from "@/lib/chart-theme";
import type { SpanDetail } from "@/lib/insight-types";

const ROW = 22;
const LABEL = 228;

export function formatDur(us: number) {
  if (us < 1000) return `${Math.round(us)} µs`;
  const ms = us / 1000;
  if (ms < 1000) return `${ms.toFixed(ms < 10 ? 2 : 1)} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

function reducedMotion() {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function Waterfall({
  spans,
  selected,
  onSelect,
}: {
  spans: SpanDetail[];
  selected: number;
  onSelect: (index: number) => void;
}) {
  const theme = useChartTheme();
  const scrollRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const liveRef = useRef<HTMLParagraphElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const scroller = scrollRef.current;
    if (!canvas || !scroller || spans.length === 0) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const paint = () => {
      const width = scroller.clientWidth;
      const height = scroller.clientHeight;
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.max(1, Math.floor(width * dpr));
      canvas.height = Math.max(1, Math.floor(height * dpr));
      canvas.style.width = `${width}px`;
      canvas.style.height = `${height}px`;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, width, height);
      ctx.font = CHART_FONT;
      const minStart = spans.reduce((min, span) => Math.min(min, span.startMs), spans[0].startMs);
      const maxEnd = spans.reduce(
        (max, span) => Math.max(max, span.startMs * 1000 + span.durationUs),
        0,
      );
      const total = Math.max(1, maxEnd - minStart * 1000);
      const barLeft = LABEL;
      const barWidth = Math.max(40, width - barLeft - 12);
      const top = scroller.scrollTop;
      const first = Math.max(0, Math.floor(top / ROW) - 1);
      const last = Math.min(spans.length, Math.ceil((top + height) / ROW) + 1);
      for (let i = first; i < last; i += 1) {
        const span = spans[i];
        const y = i * ROW - top;
        if (i === selected) {
          ctx.fillStyle = theme.band.purple;
          ctx.fillRect(0, y, width, ROW);
        } else if (i % 2 === 0) {
          ctx.fillStyle = theme.grid;
          ctx.fillRect(0, y, width, ROW);
        }
        const indent = Math.min(span.depth, 8) * 10;
        const prefix = `${span.critical ? "crit " : ""}${span.status === "error" ? "error " : ""}`;
        const label = `${prefix}${span.service} ${span.operation}`;
        ctx.fillStyle = theme.axis;
        ctx.fillText(label, 8 + indent, y + 15, LABEL - indent - 16);
        const x = barLeft + ((span.startMs * 1000 - minStart * 1000) / total) * barWidth;
        const w = Math.max(2, (span.durationUs / total) * barWidth);
        const color = span.status === "error" ? theme.colors.vermillion : theme.colors.blue;
        ctx.fillStyle = color;
        ctx.fillRect(x, y + 6, w, 10);
        if (span.critical) {
          ctx.save();
          ctx.beginPath();
          ctx.rect(x, y + 6, w, 10);
          ctx.clip();
          ctx.strokeStyle = theme.colors.orange;
          ctx.lineWidth = 1;
          for (let hatch = x - 10; hatch < x + w; hatch += 4) {
            ctx.beginPath();
            ctx.moveTo(hatch, y + 16);
            ctx.lineTo(hatch + 10, y + 6);
            ctx.stroke();
          }
          ctx.restore();
          ctx.strokeStyle = theme.colors.orange;
          ctx.strokeRect(x + 0.5, y + 6.5, Math.max(1, w - 1), 9);
        }
      }
      const current = spans[selected];
      if (current && liveRef.current) {
        liveRef.current.textContent = `${current.service} ${current.operation}, ${current.status}, ${formatDur(current.durationUs)}${current.critical ? ", on the critical path" : ""}`;
      }
    };

    const onScroll = () => {
      if (reducedMotion()) paint();
      else requestAnimationFrame(paint);
    };
    paint();
    scroller.addEventListener("scroll", onScroll, { passive: true });
    const observer = new ResizeObserver(paint);
    observer.observe(scroller);
    return () => {
      scroller.removeEventListener("scroll", onScroll);
      observer.disconnect();
    };
  }, [spans, selected, theme]);

  const move = (next: number) => {
    const index = Math.max(0, Math.min(spans.length - 1, next));
    onSelect(index);
    const scroller = scrollRef.current;
    if (!scroller) return;
    const top = index * ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    const bottom = top + ROW;
    if (bottom > scroller.scrollTop + scroller.clientHeight) {
      scroller.scrollTop = bottom - scroller.clientHeight;
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <p className="text-xs text-muted-foreground">
        Hatched bars are the critical path. Error spans are also labeled “error”.
      </p>
      <div className="relative h-[32rem] rounded-lg border border-border">
        <canvas ref={canvasRef} className="pointer-events-none absolute inset-0" aria-hidden />
        <div
          ref={scrollRef}
          className="absolute inset-0 overflow-auto focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
          tabIndex={0}
          role="group"
          aria-label="Spans in this trace. Use arrow keys to move."
          onClick={(event) => {
            const bounds = event.currentTarget.getBoundingClientRect();
            const y = event.clientY - bounds.top + event.currentTarget.scrollTop;
            move(Math.floor(y / ROW));
          }}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              move(selected + 1);
            } else if (event.key === "ArrowUp") {
              event.preventDefault();
              move(selected - 1);
            } else if (event.key === "Home") {
              event.preventDefault();
              move(0);
            } else if (event.key === "End") {
              event.preventDefault();
              move(spans.length - 1);
            }
          }}
        >
          <div style={{ height: spans.length * ROW }} />
        </div>
      </div>
      <p ref={liveRef} className="sr-only" aria-live="polite" />
    </div>
  );
}
