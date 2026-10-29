import { usePrivacy } from "@/contexts/privacy-context"
import { useLicense } from "@/contexts/license-context"
import { SyntheticPanel } from "./synthetic-panel"
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { MetricUplot } from "@/components/charts/metric-uplot";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Notice } from "@/components/status/notice";
import { infraApi, type Check, type CheckView } from "./api";
import { cell, Status } from "./resource-panel";

export function UptimePanel({ serverId }: { serverId: string }) {
  const { enabled: privateMode } = usePrivacy()
  const { license } = useLicense()
  const writable = license?.canWrite === true
  const [views, setViews] = useState<CheckView[] | null>(null);
  const [domains, setDomains] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [url, setUrl] = useState("");
  const [interval, setIntervalValue] = useState(60);
  const [allowSelfSigned, setAllowSelfSigned] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [selected, setSelected] = useState("");
  const [domainQuery, setDomainQuery] = useState("");
  const [revision, setRevision] = useState(0);
  const reload = useCallback(() => setRevision((n) => n + 1), []);
  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const [next, suggestions] = await Promise.all([
          infraApi.checks(serverId),
          infraApi.domains(serverId),
        ]);
        if (active) {
          setViews(next);
          setDomains(suggestions);
          setError(null);
        }
      } catch (e) {
        if (active) setError(String(e));
      } finally {
        if (active) timer = setTimeout(poll, 5000);
      }
    };
    void poll();
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [serverId, revision]);
  const action = async (key: string, task: () => Promise<unknown>) => {
    if (!writable || privateMode) return
    setBusy(key);
    try {
      await task();
      reload();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };
  const add = async (value: string) => {
    const check = await infraApi.saveCheck({
      id: "",
      serverId,
      url: value,
      intervalSeconds: interval,
      enabled: true,
      allowSelfSigned,
    });
    setSelected(check.id);
    setUrl("");
    toast.success("PC uptime check added");
  };
  const picked = views?.find((v) => v.check.id === selected) ?? views?.[0];
  const latest = picked?.history[picked.history.length - 1];
  const points = useMemo(
    () => (picked?.history ?? []).map((r) => ({ t: r.tsMs, v: r.totalMs })),
    [picked],
  );
  const suggestions = domains.filter(
    (domain) =>
      domain.includes(domainQuery.toLowerCase()) &&
      !views?.some((v) => v.check.url === `https://${domain}/`),
  );
  if (privateMode) return <div className="space-y-4"><SyntheticPanel serverId={serverId} /><p>Uptime target details are hidden in privacy mode</p></div>
  return (
    <Card>
      <CardHeader>
        <CardTitle>DNS and uptime from your PC</CardTitle>
        <CardDescription>
          Verified HTTP(S) GET checks run while Tracelyn is open. DNS resolves
          locally. Timings separate DNS, TCP, TLS and server wait. Redirects are
          recorded as their initial response.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <SyntheticPanel serverId={serverId} />
        {error && (
          <Notice tone="critical" title="Could not load checks">
            {error} Retry after checking local database access.
          </Notice>
        )}
        <form
          className="flex flex-wrap items-end gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            void action("add", () => add(url));
          }}
        >
          <div className="min-w-60 flex-1 space-y-2">
            <Label htmlFor="uptime-url">Check URL</Label>
            <Input
              id="uptime-url"
              type="url"
              required
              placeholder="https://example.com/health"
              value={url}
              maxLength={2048}
              onChange={(e) => setUrl(e.target.value)}
            />
          </div>
          <div className="w-36 space-y-2">
            <Label htmlFor="uptime-interval">Interval (seconds)</Label>
            <Input
              id="uptime-interval"
              type="number"
              min={15}
              max={86400}
              required
              value={interval}
              onChange={(e) => setIntervalValue(Number(e.target.value))}
            />
          </div>
          <label className="flex items-center gap-2 pb-2 text-sm" htmlFor="uptime-self-signed">
            <input
              id="uptime-self-signed"
              type="checkbox"
              checked={allowSelfSigned}
              onChange={(e) => setAllowSelfSigned(e.target.checked)}
            />
            Allow self-signed certificate
          </label>
          <Button type="submit" disabled={!writable || busy !== null}>
            {busy === "add" ? "Adding…" : "Add check"}
          </Button>
        </form>
        <p className="text-xs text-muted-foreground">
          HTTPS checks verify the certificate. Turn on the option only for a host you trust with a private certificate.
        </p>
        {domains.length > 0 ? (
          <details className="rounded-lg border border-border p-3">
            <summary className="cursor-pointer text-sm font-medium">
              Discovered domains · {suggestions.length} suggestions
            </summary>
            <div className="mt-3 space-y-3">
              <Input
                aria-label="Filter discovered domains"
                placeholder="Filter domains…"
                value={domainQuery}
                onChange={(e) => setDomainQuery(e.target.value)}
              />
              <div className="flex flex-wrap gap-2">
                {suggestions.slice(0, 20).map((domain) => (
                  <Button
                    key={domain}
                    size="sm"
                    variant="outline"
                    disabled={!writable || busy !== null}
                    onClick={() =>
                      void action(`add-${domain}`, () =>
                        add(`https://${domain}/`),
                      )
                    }
                  >
                    Add {domain}
                  </Button>
                ))}
              </div>
              <p className="text-xs text-muted-foreground">
                Showing up to 20 matching domains. New checks use the interval
                above. Wildcard and regex names need a concrete hostname.
              </p>
            </div>
          </details>
        ) : (
          <p className="text-sm text-muted-foreground">
            Domain suggestions appear after web configuration discovery. You can
            add a URL now.
          </p>
        )}
        {!views && !error && <Skeleton className="h-40 w-full" />}
        {views?.length === 0 && (
          <Notice title="No uptime checks yet">
            Add a URL or select a discovered domain. History is stored locally
            in SQLite, with a maximum of 32 checks per server.
          </Notice>
        )}
        {views && views.length > 0 && (
          <>
            <div
              className="overflow-auto"
              tabIndex={0}
              role="region"
              aria-label="Uptime checks table"
            >
              <table className="w-full text-left text-sm">
                <caption className="sr-only">
                  Configured uptime checks and latest results
                </caption>
                <thead>
                  <tr className="border-b border-border text-xs text-muted-foreground">
                    {[
                      "URL",
                      "State",
                      "HTTP",
                      "Interval",
                      "Latest check",
                      "Actions",
                    ].map((h) => (
                      <th key={h} scope="col" className="py-2 pr-4 font-medium">
                        {h}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {views.map(({ check, history }) => {
                    const last = history[history.length - 1];
                    return (
                      <tr
                        key={check.id}
                        className="border-b border-border/60 align-top"
                      >
                        <th
                          scope="row"
                          className="max-w-sm break-all py-3 pr-4 font-normal"
                        >
                          <button
                            className="min-h-6 text-left font-medium underline decoration-border underline-offset-4 hover:text-primary"
                            onClick={() => setSelected(check.id)}
                          >
                            {check.url}
                          </button>
                          {last?.error && (
                            <p className="mt-1 text-xs text-muted-foreground">
                              {last.error}
                            </p>
                          )}
                        </th>
                        <td className="py-3 pr-4">
                          <Status
                            value={
                              !check.enabled
                                ? "paused"
                                : !last
                                  ? "loading"
                                  : last.ok
                                    ? "up"
                                    : "down"
                            }
                          />
                        </td>
                        <td className="py-3 pr-4 tabular-nums">
                          {last?.statusCode ?? "—"}
                        </td>
                        <td className="py-3 pr-4 tabular-nums">
                          {check.intervalSeconds}s
                        </td>
                        <td className="whitespace-nowrap py-3 pr-4 text-xs">
                          {last
                            ? new Date(last.tsMs).toLocaleString()
                            : "Awaiting first check"}
                        </td>
                        <td className="py-3">
                          <div className="flex gap-2">
                            <Button
                              size="sm"
                              variant="outline"
                              aria-label={`Run check for ${check.url}`}
                              disabled={!writable || busy !== null}
                              onClick={() =>
                                void action(check.id, () =>
                                  infraApi.runCheck(serverId, check.id),
                                )
                              }
                            >
                              Run
                            </Button>
                            <Button
                              size="sm"
                              variant="outline"
                              aria-label={`${check.enabled ? "Pause" : "Resume"} ${check.url}`}
                              disabled={!writable || busy !== null}
                              onClick={() =>
                                void action(check.id, () =>
                                  infraApi.saveCheck({
                                    ...check,
                                    enabled: !check.enabled,
                                  }),
                                )
                              }
                            >
                              {check.enabled ? "Pause" : "Resume"}
                            </Button>
                            <Button
                              size="sm"
                              variant="ghost"
                              aria-label={`Remove ${check.url}`}
                              disabled={!writable || busy !== null}
                              onClick={() =>
                                void action(check.id, () =>
                                  infraApi.deleteCheck(serverId, check.id),
                                )
                              }
                            >
                              Remove
                            </Button>
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            {picked && (
              <CheckDetail
                view={picked}
                onSave={(check) =>
                  void action(check.id, () => infraApi.saveCheck(check))
                }
                busy={!writable || busy !== null}
              />
            )}
            {picked && latest && (
              <section
                className="space-y-4"
                aria-label="Selected uptime check history"
              >
                <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
                  {[
                    ["DNS", latest.dnsMs],
                    ["TCP connect", latest.connectMs],
                    ["TLS handshake", latest.tlsMs],
                    ["Server wait", latest.ttfbMs],
                    ["Total (incl. DNS)", latest.totalMs],
                  ].map(([name, value]) => (
                    <div
                      key={String(name)}
                      className="rounded-lg border border-border bg-muted/30 p-3"
                    >
                      <p className="text-xs text-muted-foreground">{name}</p>
                      <p className="mt-1 font-mono text-lg">
                        {cell(value, "ms")}
                      </p>
                    </div>
                  ))}
                </div>
                <p className="text-xs text-muted-foreground">
                  {picked.check.url} · {latest.addresses.join(", ")} · Sampled
                  availability:{" "}
                  {(
                    (100 * picked.history.filter((r) => r.ok).length) /
                    picked.history.length
                  ).toFixed(1)}
                  % across the latest {picked.history.length} checks. Scheduling
                  may be delayed when all four PC workers are busy.
                </p>
                <MetricUplot
                  title="HTTP check duration"
                  points={points}
                  color="blue"
                  formatValue={(v) => `${v.toFixed(0)} ms`}
                />
                <details>
                  <summary className="cursor-pointer text-sm font-medium">
                    Recent results and timing history
                  </summary>
                  <div
                    className="mt-3 overflow-auto"
                    tabIndex={0}
                    role="region"
                    aria-label="Uptime timing history table"
                  >
                    <table className="w-full text-left text-xs">
                      <caption className="sr-only">
                        Last 20 HTTP results for {picked.check.url}
                      </caption>
                      <thead>
                        <tr className="border-b border-border">
                          {[
                            "Time",
                            "State",
                            "HTTP",
                            "DNS",
                            "TCP",
                            "TLS",
                            "Wait",
                            "Total",
                          ].map((h) => (
                            <th scope="col" className="py-2 pr-3" key={h}>
                              {h}
                            </th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {picked.history
                          .slice(-20)
                          .reverse()
                          .map((r) => (
                            <tr
                              key={r.tsMs}
                              className="border-b border-border/60"
                            >
                              <th scope="row" className="py-2 pr-3 font-normal">
                                {new Date(r.tsMs).toLocaleString()}
                              </th>
                              <td className="py-2 pr-3">
                                <Status value={r.ok ? "up" : "down"} />
                              </td>
                              <td>{r.statusCode ?? "—"}</td>
                              {[
                                r.dnsMs,
                                r.connectMs,
                                r.tlsMs,
                                r.ttfbMs,
                                r.totalMs,
                              ].map((v, i) => (
                                <td className="py-2 pr-3 tabular-nums" key={i}>
                                  {cell(v, "ms")}
                                </td>
                              ))}
                            </tr>
                          ))}
                      </tbody>
                    </table>
                  </div>
                </details>
              </section>
            )}
          </>
        )}
      </CardContent>
    </Card>
  );
}
function CheckDetail({
  view,
  onSave,
  busy,
}: {
  view: CheckView;
  onSave: (check: Check) => void;
  busy: boolean;
}) {
  const [interval, setIntervalValue] = useState(view.check.intervalSeconds);
  const [url, setUrl] = useState(view.check.url);
  useEffect(() => {
    setIntervalValue(view.check.intervalSeconds);
    setUrl(view.check.url);
  }, [view.check.id, view.check.intervalSeconds, view.check.url]);
  return (
    <form
      className="flex flex-wrap items-end gap-3 rounded-lg border border-border p-4"
      onSubmit={(e) => {
        e.preventDefault();
        onSave({ ...view.check, url, intervalSeconds: interval });
      }}
    >
      <div className="min-w-60 flex-1 space-y-2">
        <Label htmlFor="edit-check-url">Selected check URL</Label>
        <Input
          id="edit-check-url"
          type="url"
          value={url}
          required
          onChange={(e) => setUrl(e.target.value)}
        />
      </div>
      <div className="w-36 space-y-2">
        <Label htmlFor="edit-check-interval">Interval (seconds)</Label>
        <Input
          id="edit-check-interval"
          type="number"
          min={15}
          max={86400}
          value={interval}
          required
          onChange={(e) => setIntervalValue(Number(e.target.value))}
        />
      </div>
      <Button size="sm" type="submit" disabled={busy}>
        Update check
      </Button>
    </form>
  );
}
