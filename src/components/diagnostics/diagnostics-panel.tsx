import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { HowToFix } from "@/components/setup/how-to-fix";
import { buttonVariants } from "@/components/ui/button";
import { getServerDiagnostics } from "@/lib/api";
import { cn } from "@/lib/utils";
import type { DiagnosticEntry } from "@/lib/apps-types";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";

const TONE = {
  ok: "bg-status-ok",
  warning: "bg-status-warning",
  error: "bg-status-critical",
} as const;

export function DiagnosticsPanel({ serverId }: { serverId: string }) {
  const [rows, setRows] = useState<DiagnosticEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const load = () => {
      void getServerDiagnostics(serverId)
        .then((next) => {
          if (!cancelled) {
            setRows(next);
            setError(null);
          }
        })
        .catch((err) => {
          if (!cancelled) setError(String(err));
        });
    };
    load();
    const timer = setInterval(load, 10_000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [serverId]);

  return (
    <Card>
      <CardHeader className="flex flex-row items-start justify-between gap-3">
        <div>
          <CardTitle className="text-sm">Diagnostics</CardTitle>
          <CardDescription>
            Each collector reports OK, a warning, or an error with a one-line fix.
          </CardDescription>
        </div>
        <Link
          to={`/setup?server=${encodeURIComponent(serverId)}`}
          className={cn(buttonVariants({ variant: "outline", size: "sm" }), "shrink-0")}
        >
          Setup guide
        </Link>
      </CardHeader>
      <CardContent>
        {error && <p className="text-sm text-status-critical">{error}</p>}
        {rows === null && !error ? (
          <Skeleton className="h-24 w-full" />
        ) : rows && rows.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No diagnostics yet. Connect the server and wait for the first collector pass.
          </p>
        ) : (
          <ul className="flex flex-col gap-2">
            {rows?.map((row) => (
              <li
                key={row.collector}
                className="flex items-start gap-3 rounded-lg border border-border px-3 py-2"
              >
                <span
                  className={cn("mt-1.5 size-2 shrink-0 rounded-full", TONE[row.status])}
                  aria-hidden
                />
                <div className="min-w-0">
                  <p className="text-sm">
                    <span className="font-medium">{row.collector}</span>
                    <span className="sr-only"> {row.status}. </span>
                    <span className="text-muted-foreground"> — {row.message}</span>
                  </p>
                  {row.fixHint && (
                    <div className="mt-1 flex flex-wrap items-center gap-2">
                      <p className="text-xs text-muted-foreground">{row.fixHint}</p>
                      <HowToFix text={`${row.collector}: ${row.message}. ${row.fixHint}`} />
                    </div>
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
