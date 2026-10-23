const COLORS: Record<string, string> = {
  "2xx": "var(--color-status-ok)",
  "3xx": "var(--color-status-info)",
  "4xx": "var(--color-status-warning)",
  "5xx": "var(--color-status-critical)",
};

interface StatusStackBarProps {
  breakdown: Record<string, number>;
}

export function StatusStackBar({ breakdown }: StatusStackBarProps) {
  const total = Object.values(breakdown).reduce((a, b) => a + b, 0) || 1;
  const entries = Object.entries(breakdown).sort(([a], [b]) => a.localeCompare(b));

  return (
    <div className="flex flex-col gap-2">
      <div className="flex h-3 w-full overflow-hidden rounded-full bg-muted">
        {entries.map(([key, count]) => (
          <div
            key={key}
            className="h-full transition-all duration-200"
            style={{
              width: `${(count / total) * 100}%`,
              backgroundColor: COLORS[key] ?? "var(--color-muted-foreground)",
            }}
            title={`${key}: ${count}`}
          />
        ))}
      </div>
      <div className="flex flex-wrap gap-3 text-xs text-muted-foreground">
        {entries.map(([key, count]) => (
          <span key={key} className="flex items-center gap-1.5">
            <span
              className="size-2 rounded-full"
              style={{ backgroundColor: COLORS[key] }}
            />
            <span className="font-mono tabular-nums">{key}</span>
            <span className="font-mono tabular-nums text-foreground">
              {((count / total) * 100).toFixed(1)}%
            </span>
          </span>
        ))}
      </div>
    </div>
  );
}
