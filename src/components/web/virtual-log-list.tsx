import { useVirtualizer } from "@tanstack/react-virtual";
import { useRef } from "react";
import type { LiveLogLine } from "@/lib/web-traffic-types";
import { cn } from "cn";

interface VirtualLogListProps {
  lines: LiveLogLine[];
  onSelect?: (line: LiveLogLine) => void;
}

function statusColor(status: number) {
  if (status >= 500) return "text-status-critical";
  if (status >= 400) return "text-status-warning";
  if (status >= 300) return "text-status-info";
  return "text-status-ok";
}

export function VirtualLogList({ lines, onSelect }: VirtualLogListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: lines.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 28,
    overscan: 12,
  });

  return (
    <div
      ref={parentRef}
      className="h-[min(420px,50vh)] overflow-auto rounded-lg border border-border bg-card/40 font-mono text-xs"
    >
      <div
        style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}
      >
        {virtualizer.getVirtualItems().map((item) => {
          const line = lines[item.index];
          return (
            <button
              key={line.id}
              type="button"
              className={cn(
                "absolute left-0 flex w-full items-center gap-2 px-3 text-left hover:bg-muted/50",
                statusColor(line.status),
              )}
              style={{
                height: `${item.size}px`,
                transform: `translateY(${item.start}px)`,
              }}
              onClick={() => onSelect?.(line)}
            >
              <span className="w-10 shrink-0 tabular-nums opacity-70">
                {line.status}
              </span>
              <span className="w-12 shrink-0">{line.method}</span>
              <span className="min-w-0 flex-1 truncate text-foreground">
                {line.path}
              </span>
              <span className="hidden shrink-0 text-muted-foreground sm:inline">
                {line.remoteAddr}
              </span>
              {line.latencyMs !== undefined && (
                <span className="w-14 shrink-0 text-right tabular-nums text-muted-foreground">
                  {line.latencyMs.toFixed(0)}ms
                </span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
