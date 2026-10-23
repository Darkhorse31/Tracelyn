import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import type { RedisSlowlogEntry } from "@/lib/redis-types";

function formatDuration(micros: number): string {
  if (micros >= 1_000_000) return `${(micros / 1_000_000).toFixed(2)} s`;
  if (micros >= 1000) return `${(micros / 1000).toFixed(1)} ms`;
  return `${micros} µs`;
}

export function RedisSlowlogTable({ entries }: { entries: RedisSlowlogEntry[] }) {
  return (
    <Card className="flex min-h-[320px] flex-col">
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-medium">Slow log</CardTitle>
      </CardHeader>
      <CardContent className="flex-1 p-0">
        {entries.length === 0 ? (
          <p className="px-6 pb-6 text-sm text-muted-foreground">
            No slow queries in the current window.
          </p>
        ) : (
          <ScrollArea className="h-[min(420px,50vh)]">
            <table className="w-full text-sm">
              <thead className="sticky top-0 bg-card">
                <tr className="border-b border-border text-left text-muted-foreground">
                  <th className="px-4 py-2 font-medium">ID</th>
                  <th className="px-4 py-2 font-medium">Time</th>
                  <th className="px-4 py-2 font-medium">Duration</th>
                  <th className="px-4 py-2 font-medium">Client</th>
                  <th className="px-4 py-2 font-medium">Command</th>
                </tr>
              </thead>
              <tbody>
                {entries.map((row) => (
                  <tr
                    key={row.id}
                    className="border-b border-border/60 last:border-0 hover:bg-muted/40"
                  >
                    <td className="px-4 py-2 font-mono tabular-nums text-muted-foreground">
                      {row.id}
                    </td>
                    <td className="px-4 py-2 font-mono tabular-nums text-xs text-muted-foreground">
                      {new Date(row.timestampUnix * 1000).toLocaleString()}
                    </td>
                    <td className="px-4 py-2 font-mono tabular-nums">
                      {formatDuration(row.durationMicros)}
                    </td>
                    <td className="px-4 py-2 font-mono text-xs">{row.client}</td>
                    <td className="max-w-[min(480px,40vw)] truncate px-4 py-2 font-mono text-xs">
                      {row.command}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </ScrollArea>
        )}
      </CardContent>
    </Card>
  );
}
