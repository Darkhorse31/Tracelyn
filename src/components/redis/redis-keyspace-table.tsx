import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { RedisDbKeyspace } from "@/lib/redis-types";

export function RedisKeyspaceTable({ keyspace }: { keyspace: RedisDbKeyspace[] }) {
  return (
    <Card className="h-full">
      <CardHeader>
        <CardTitle className="text-sm">Keys per database</CardTitle>
      </CardHeader>
      <CardContent className="overflow-x-auto">
        {keyspace.length === 0 ? (
          <p className="text-sm text-muted-foreground">No keyspace data.</p>
        ) : (
          <table className="w-full text-sm">
            <caption className="sr-only">Keys, expiring keys and average TTL per database</caption>
            <thead>
              <tr className="border-b border-border text-left text-xs text-muted-foreground">
                <th scope="col" className="pb-2 font-medium">DB</th>
                <th scope="col" className="pb-2 text-right font-medium">Keys</th>
                <th scope="col" className="pb-2 text-right font-medium">Expires</th>
                <th scope="col" className="pb-2 text-right font-medium">Avg TTL</th>
              </tr>
            </thead>
            <tbody className="font-mono">
              {keyspace.map((row) => (
                <tr key={row.db} className="border-b border-border/60 last:border-0">
                  <th scope="row" className="py-2 text-left font-normal">db{row.db}</th>
                  <td className="py-2 text-right">{row.keys.toLocaleString()}</td>
                  <td className="py-2 text-right">{row.expires.toLocaleString()}</td>
                  <td className="py-2 text-right text-muted-foreground">
                    {row.avgTtlMs > 0 ? `${Math.round(row.avgTtlMs / 1000).toLocaleString()} s` : "—"}
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
