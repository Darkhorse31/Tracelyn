import { useVirtualizer } from "@tanstack/react-virtual";
import { useRef } from "react";
import { usePrivacy } from "@/contexts/privacy-context";
import type { LiveLogLine } from "@/lib/web-traffic-types";
import { cn } from "cn";

interface VirtualLogListProps {
  lines: LiveLogLine[];
}

function statusColor(status: number) {
  if (status >= 500) return "text-status-critical";
  if (status >= 400) return "text-status-warning";
  if (status >= 300) return "text-status-info";
  return "text-status-ok";
}

export function VirtualLogList({ lines }: VirtualLogListProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const { enabled, mask } = usePrivacy();
  const virtualizer = useVirtualizer({
    count: lines.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 28,
    overscan: 12,
  });

  return (
    <div
      ref={parentRef}
      role="region"
      aria-label="Access log lines"
      tabIndex={0}
      className="h-[min(420px,50vh)] overflow-auto rounded-lg border border-border bg-background/40 font-mono text-xs focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
    >
      {lines.length === 0 ? (
        <p className="flex h-full items-center justify-center font-sans text-sm text-muted-foreground">
          No log lines match the current filters.
        </p>
      ) : (
        <div style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const line = lines[item.index];
            return (
              <div
                key={line.id}
                className="absolute left-0 flex w-full items-center gap-3 px-3 hover:bg-muted/50"
                style={{ height: `${item.size}px`, transform: `translateY(${item.start}px)` }}
              >
                <span className={cn("w-8 shrink-0 font-medium", statusColor(line.status))}>
                  {line.status}
                </span>
                <span className="w-14 shrink-0 text-muted-foreground">{line.method}</span>
                <span className={cn("min-w-0 flex-1 truncate", enabled && "privacy-blur")} title={mask(line.path)}>
                  {mask(line.path)}
                </span>
                <span className={cn("hidden w-32 shrink-0 truncate text-muted-foreground sm:inline", enabled && "privacy-blur")} data-privacy-mask={enabled ? "" : undefined}>
                  {mask(line.remoteAddr)}
                </span>
                <span className="w-16 shrink-0 text-right text-muted-foreground">
                  {line.latencyMs !== undefined ? `${line.latencyMs.toFixed(0)} ms` : "—"}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
