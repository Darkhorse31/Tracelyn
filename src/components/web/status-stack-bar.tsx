import { cn } from "@/lib/utils";

const COLORS: Record<string, string> = {
  "2xx": "var(--color-status-ok)",
  "3xx": "var(--color-status-info)",
  "4xx": "var(--color-status-warning)",
  "5xx": "var(--color-status-critical)",
};

interface StatusStackBarProps {
  breakdown: Record<string, number>;
  /** Bar only, for table rows. The summary is still exposed to assistive tech. */
  compact?: boolean;
}

export function StatusStackBar({ breakdown, compact = false }: StatusStackBarProps) {
  const entries = Object.entries(breakdown)
    .filter(([, count]) => count > 0)
    .sort(([a], [b]) => a.localeCompare(b));
  const total = entries.reduce((sum, [, count]) => sum + count, 0);
  const pct = (count: number) => (total ? (count / total) * 100 : 0);
  const summary =
    total === 0
      ? "No responses"
      : entries.map(([key, count]) => `${key} ${pct(count).toFixed(1)}%`).join(", ");

  return (
    <div className="flex flex-col gap-2">
      <div
        role="img"
        aria-label={`Status codes: ${summary}`}
        title={compact ? summary : undefined}
        className={cn(
          "flex w-full gap-px overflow-hidden rounded-full bg-muted",
          compact ? "h-2" : "h-3",
        )}
      >
        {entries.map(([key, count]) => (
          <div
            key={key}
            className="h-full transition-[width] duration-200 motion-reduce:transition-none"
            style={{
              width: `${pct(count)}%`,
              backgroundColor: COLORS[key] ?? "var(--color-muted-foreground)",
            }}
          />
        ))}
      </div>
      {!compact && (
        <ul className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground" aria-hidden>
          {entries.length === 0 && <li>No responses yet</li>}
          {entries.map(([key, count]) => (
            <li key={key} className="flex items-center gap-1.5">
              <span
                className="size-2 rounded-full"
                style={{ backgroundColor: COLORS[key] ?? "var(--color-muted-foreground)" }}
              />
              <span className="font-mono">{key}</span>
              <span className="font-mono text-foreground">{pct(count).toFixed(1)}%</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
