import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { RedisDbKeyspace } from "@/lib/redis-types";

export function RedisKeyspaceTable({ keyspace }: { keyspace: RedisDbKeyspace[] }) {
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-medium">Keys per database</CardTitle>
      </CardHeader>
      <CardContent className="overflow-x-auto">
        {keyspace.length === 0 ? (
          <p className="text-sm text-muted-foreground">No keyspace data.</p>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border text-left text-muted-foreground">
                <th className="pb-2 pr-4 font-medium">DB</th>
                <th className="pb-2 pr-4 font-medium">Keys</th>
                <th className="pb-2 pr-4 font-medium">Expires</th>
                <th className="pb-2 font-medium">Avg TTL</th>
              </tr>
            </thead>
            <tbody>
              {keyspace.map((row) => (
                <tr key={row.db} className="border-b border-border/60 last:border-0">
                  <td className="py-2 pr-4 font-mono tabular-nums">db{row.db}</td>
                  <td className="py-2 pr-4 font-mono tabular-nums">
                    {row.keys.toLocaleString()}
                  </td>
                  <td className="py-2 pr-4 font-mono tabular-nums">
                    {row.expires.toLocaleString()}
                  </td>
                  <td className="py-2 font-mono tabular-nums text-muted-foreground">
                    {row.avgTtlMs > 0
                      ? `${Math.round(row.avgTtlMs / 1000)}s`
                      : "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </CardContent>
    </Card>
  );
}
