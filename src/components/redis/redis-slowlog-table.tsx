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
    <Card className="h-full">
      <CardHeader>
        <CardTitle className="text-sm">Slow log</CardTitle>
      </CardHeader>
      <CardContent className="flex-1 px-0">
        {entries.length === 0 ? (
          <p className="px-4 text-sm text-muted-foreground">No slow queries in the current window.</p>
        ) : (
          <ScrollArea className="h-[min(420px,50vh)]">
            <table className="w-full table-fixed text-sm">
              <caption className="sr-only">Redis slow log, most recent first</caption>
              <thead className="sticky top-0 bg-card">
                <tr className="border-b border-border text-left text-xs text-muted-foreground">
                  <th scope="col" className="w-16 px-4 py-2 font-medium">ID</th>
                  <th scope="col" className="w-44 px-2 py-2 font-medium">Time</th>
                  <th scope="col" className="w-24 px-2 py-2 text-right font-medium">Duration</th>
                  <th scope="col" className="w-36 px-2 py-2 font-medium">Client</th>
                  <th scope="col" className="px-4 py-2 font-medium">Command</th>
                </tr>
              </thead>
              <tbody className="font-mono text-xs">
                {entries.map((row) => (
                  <tr key={row.id} className="border-b border-border/60 last:border-0 hover:bg-muted/40">
                    <td className="px-4 py-2 text-muted-foreground">{row.id}</td>
                    <td className="px-2 py-2 text-muted-foreground">
                      {new Date(row.timestampUnix * 1000).toLocaleString()}
                    </td>
                    <td className="px-2 py-2 text-right">{formatDuration(row.durationMicros)}</td>
                    <td className="truncate px-2 py-2" title={row.client}>{row.client}</td>
                    <td className="truncate px-4 py-2" title={row.command}>{row.command}</td>
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
