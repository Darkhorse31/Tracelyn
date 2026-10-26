import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { Timer } from "lucide-react";
import { toast } from "sonner";
import { EmptyState, Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { useServersContext } from "@/contexts/servers-context";
import { getInsightSetup, listSlowQueries, setSlowlogPath } from "@/lib/api";
import type { InsightSetup, SlowQueryRow } from "@/lib/insight-types";

const NAMES: Record<string, string> = {
  mysql: "MySQL / MariaDB",
  postgres: "PostgreSQL",
  mongo: "MongoDB",
};

export function SlowQueriesPage() {
  const { servers, selectedServerId } = useServersContext();
  const [rows, setRows] = useState<SlowQueryRow[] | null>(null);
  const [setup, setSetup] = useState<InsightSetup | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [engine, setEngine] = useState("");
  const server = servers.find((item) => item.id === selectedServerId);

  const load = () => {
    if (!selectedServerId) return;
    void Promise.all([listSlowQueries(selectedServerId, engine || undefined), getInsightSetup(selectedServerId)])
      .then(([queries, nextSetup]) => {
        setRows(queries);
        setSetup(nextSetup);
        setError(null);
      })
      .catch((err) => setError(String(err)));
  };

  useEffect(() => {
    load();
    const timer = setInterval(load, 5000);
    return () => clearInterval(timer);
  }, [selectedServerId, engine]);

  if (!server || !selectedServerId) {
    return (
      <EmptyState
        icon={Timer}
        title="No server selected"
        description="Select a server to read MySQL, PostgreSQL, and MongoDB slow logs."
      />
    );
  }

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h2 className="text-lg font-semibold tracking-tight">Slow queries</h2>
        <p className="text-sm text-muted-foreground">
          {server.name}. Statements are grouped after literals are replaced with ?. Tracelyn only reads the log.
        </p>
      </div>
      {error && (
        <Notice tone="critical" title="Could not load slow queries">
          {error}
        </Notice>
      )}
      <div className="flex flex-wrap gap-2" role="group" aria-label="Database engine">
        {["", "mysql", "postgres", "mongo"].map((name) => (
          <Button
            key={name || "all"}
            type="button"
            size="sm"
            variant={engine === name ? "secondary" : "outline"}
            aria-pressed={engine === name}
            onClick={() => setEngine(name)}
          >
            {name ? NAMES[name] : "All"}
          </Button>
        ))}
      </div>
      {!rows && !error && <Skeleton className="h-32 w-full" />}
      {rows && rows.length === 0 && (
        <Notice title="No slow statements stored yet">
          If logging is off, the cards below say which setting to turn on. If it is on, no statement has crossed the threshold since Tracelyn started reading.
        </Notice>
      )}
      {rows && rows.length > 0 && (
        <div className="overflow-x-auto">
          <table className="w-full min-w-[860px] text-left text-sm">
            <caption className="sr-only">Slow queries grouped by fingerprint</caption>
            <thead className="text-xs text-muted-foreground">
              <tr className="border-b border-border">
                <th scope="col" className="pb-2 font-medium">Engine</th>
                <th scope="col" className="pb-2 font-medium">Query</th>
                <th scope="col" className="pb-2 text-right font-medium">Count</th>
                <th scope="col" className="pb-2 text-right font-medium">Avg</th>
                <th scope="col" className="pb-2 text-right font-medium">p95</th>
                <th scope="col" className="pb-2 text-right font-medium">Rows</th>
                <th scope="col" className="pb-2 font-medium">Last seen</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={`${row.engine}-${row.fingerprint}`} className="border-b border-border/60 align-top">
                  <td className="py-2 pr-3">{NAMES[row.engine] ?? row.engine}</td>
                  <td className="max-w-md py-2 pr-3">
                    <pre className="whitespace-pre-wrap font-mono text-xs">{row.normalized}</pre>
                    {row.appId && (
                      <Link className="text-xs text-primary underline-offset-2 hover:underline" to={`/apps/${encodeURIComponent(row.appId)}`}>
                        Application
                      </Link>
                    )}
                  </td>
                  <td className="py-2 pr-3 text-right tabular-nums">{row.count}</td>
                  <td className="py-2 pr-3 text-right tabular-nums">{row.avgMs.toFixed(1)} ms</td>
                  <td className="py-2 pr-3 text-right tabular-nums">{row.p95Ms.toFixed(1)} ms</td>
                  <td className="py-2 pr-3 text-right tabular-nums">{row.rowsExaminedAvg.toFixed(0)}</td>
                  <td className="py-2 whitespace-nowrap">{new Date(row.lastSeenMs).toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <div className="grid gap-4 lg:grid-cols-3">
        {(setup?.engines ?? []).map((item) => (
          <EngineCard
            key={item.engine}
            serverId={selectedServerId}
            engine={item.engine}
            loggingOn={item.loggingOn}
            path={item.path ?? ""}
            hint={item.hint}
            snippet={item.snippet}
            onSaved={load}
          />
        ))}
      </div>
    </div>
  );
}

function EngineCard({
  serverId,
  engine,
  loggingOn,
  path,
  hint,
  snippet,
  onSaved,
}: {
  serverId: string;
  engine: string;
  loggingOn: boolean;
  path: string;
  hint: string;
  snippet: string;
  onSaved: () => void;
}) {
  const [value, setValue] = useState(path);
  const [busy, setBusy] = useState(false);
  useEffect(() => setValue(path), [path]);
  return (
    <section className="flex flex-col gap-3 rounded-lg border border-border p-4">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-sm font-semibold">{NAMES[engine] ?? engine}</h3>
        <span className="text-xs uppercase tracking-wide text-muted-foreground">{loggingOn ? "logging on" : "logging off"}</span>
      </div>
      <p className="text-sm text-muted-foreground">{hint}</p>
      <div className="flex items-center justify-between gap-2">
        <h4 className="text-xs font-medium">Setting to enable</h4>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => {
            void navigator.clipboard.writeText(snippet);
            toast.success("Copied database setting");
          }}
        >
          Copy
        </Button>
      </div>
      <pre className="max-h-36 overflow-auto rounded-md bg-muted/40 p-3 font-mono text-xs">{snippet}</pre>
      <form
        className="flex flex-col gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          setBusy(true);
          void setSlowlogPath(serverId, engine, value.trim())
            .then(() => {
              toast.success("Log path saved");
              onSaved();
            })
            .catch((err) => toast.error(String(err)))
            .finally(() => setBusy(false));
        }}
      >
        <Label htmlFor={`slow-path-${engine}`}>Log path</Label>
        <Input id={`slow-path-${engine}`} value={value} onChange={(event) => setValue(event.target.value)} className="font-mono" placeholder="/var/log/..." />
        <Button type="submit" size="sm" disabled={busy}>
          Save path
        </Button>
      </form>
    </section>
  );
}
