import { useCallback, useEffect, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { Button, buttonVariants } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Masked, usePrivacy } from "@/contexts/privacy-context";
import { useServersContext } from "@/contexts/servers-context";
import { runSetupChecks, type SetupCheck } from "@/lib/api";
import { cn } from "@/lib/utils";

const STATUS_LABEL: Record<string, string> = {
  pass: "Pass",
  warning: "Warning",
  fail: "Fail",
  pending: "Not checked",
};

const STATUS_CLASS: Record<string, string> = {
  pass: "bg-status-ok",
  warning: "bg-status-warning",
  fail: "bg-status-critical",
  pending: "bg-muted-foreground",
};

export function SetupPage() {
  const { servers, selectedServerId, setSelectedServerId } = useServersContext();
  const { mask } = usePrivacy();
  const [params] = useSearchParams();
  const step = params.get("step");
  const [checks, setChecks] = useState<SetupCheck[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const server = servers.find((s) => s.id === selectedServerId);

  useEffect(() => {
    const requested = params.get("server");
    if (requested && requested !== selectedServerId) setSelectedServerId(requested);
  }, [params, selectedServerId, setSelectedServerId]);

  const load = useCallback(
    async (which: string) => {
      if (!selectedServerId) return;
      setBusy(which);
      setError(null);
      try {
        const next = await runSetupChecks(selectedServerId);
        setChecks(next);
      } catch (err) {
        setError(String(err));
      } finally {
        setBusy(null);
      }
    },
    [selectedServerId],
  );

  useEffect(() => {
    void load("all");
  }, [load]);

  useEffect(() => {
    if (!step || !checks) return;
    const el = document.getElementById(`setup-${step}`);
    el?.scrollIntoView({ block: "center" });
    if (el instanceof HTMLElement) el.focus();
  }, [step, checks]);

  const summary = checks
    ? `${checks.filter((c) => c.status === "pass").length} passed, ${
        checks.filter((c) => c.status === "warning").length
      } warnings, ${checks.filter((c) => c.status === "fail").length} failed`
    : "";

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold tracking-tight">Setup guide</h2>
          <p className="mt-1 max-w-xl text-sm text-muted-foreground">
            Read-only checks for the access Tracelyn needs
            {server ? ` on ${server.name}` : ""}. Commands are copied for you to run. Tracelyn
            does not change the server.
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Link to="/how-it-works" className={cn(buttonVariants({ variant: "outline", size: "sm" }))}>
            How Tracelyn works
          </Link>
          <Button type="button" size="sm" onClick={() => void load("all")} disabled={!server || busy !== null}>
            {busy === "all" ? "Checking…" : "Re-check all"}
          </Button>
        </div>
      </div>

      {!server ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">No server selected</CardTitle>
            <CardDescription>
              Add a server on Overview, connect it, then come back. The guide uses that server&apos;s SSH user in the commands.
            </CardDescription>
          </CardHeader>
        </Card>
      ) : (
        <>
          <p className="sr-only" aria-live="polite">
            {busy ? "Running read-only checks." : summary}
          </p>
          {error && (
            <p role="alert" className="text-sm text-status-critical">
              {error}
            </p>
          )}
          {checks === null && !error ? (
            <Skeleton className="h-40 w-full" />
          ) : (
            <ul className="flex flex-col gap-3" aria-label="Setup checks">
              {checks?.map((check) => (
                <li key={check.id}>
                  <Card
                    id={`setup-${check.id}`}
                    tabIndex={-1}
                    className={cn("scroll-mt-6 outline-none focus-visible:ring-2 focus-visible:ring-ring", step === check.id && "ring-2 ring-ring")}
                  >
                    <CardHeader className="pb-2">
                      <CardTitle className="flex items-center gap-2 text-sm">
                        <span className={cn("size-2 shrink-0 rounded-full", STATUS_CLASS[check.status] ?? STATUS_CLASS.pending)} aria-hidden />
                        {check.title}
                        <span className="font-normal text-muted-foreground">
                          {STATUS_LABEL[check.status] ?? check.status}
                        </span>
                      </CardTitle>
                    </CardHeader>
                    <CardContent className="flex flex-col gap-3">
                      <p className="text-sm text-muted-foreground">{check.explanation}</p>
                      {check.details.length > 0 && (
                        <ul className="flex flex-col gap-1 font-mono text-xs text-muted-foreground">
                          {check.details.map((line) => (
                            <li key={line}><Masked text={line} /></li>
                          ))}
                        </ul>
                      )}
                      {check.command && (
                        <div className="flex flex-wrap items-start gap-2">
                          <pre className="min-w-0 flex-1 overflow-x-auto rounded-md border border-border bg-muted/40 p-2 font-mono text-xs">
                            <Masked text={check.command ?? ""} />
                          </pre>
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => {
                              void navigator.clipboard.writeText(mask(check.command ?? ""));
                              toast.success("Command copied");
                            }}
                          >
                            Copy
                          </Button>
                        </div>
                      )}
                      <div>
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          disabled={busy !== null}
                          onClick={() => void load(check.id)}
                        >
                          {busy === check.id ? "Checking…" : "Re-check"}
                        </Button>
                      </div>
                    </CardContent>
                  </Card>
                </li>
              ))}
            </ul>
          )}
          {summary && (
            <p className="text-sm text-muted-foreground" aria-hidden>
              {summary}
            </p>
          )}
        </>
      )}
    </div>
  );
}
