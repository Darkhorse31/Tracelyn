import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Link, useNavigate } from "react-router-dom";
import {
  ArrowRight,
  Check,
  Circle,
  CircleAlert,
  Loader2,
  Server,
  Sparkles,
} from "lucide-react";
import { toast } from "sonner";
import { ServerFormDialog } from "@/components/servers/server-form-dialog";
import { ServerImportDialog } from "@/components/servers/server-import-dialog";
import { Button, buttonVariants } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { SimpleSelect } from "@/components/ui/select";
import { Label } from "@/components/ui/label";
import { Notice } from "@/components/status/notice";
import { PageSkeleton } from "@/components/status/page-skeleton";
import { useServersContext } from "@/contexts/servers-context";
import { Masked, usePrivacy } from "@/contexts/privacy-context";
import {
  useRememberedState,
  writePreference,
} from "@/hooks/use-remembered-state";
import type { SetupCheck } from "@/lib/api";
import type { TracelynApp } from "@/lib/apps-types";
import { cn } from "@/lib/utils";

const STEPS = [
  "Welcome",
  "Add server",
  "Connect test",
  "Setup checks",
  "Monitor",
  "Done",
];
const CONNECTION_CHECKS = [
  "Check saved connection details",
  "Resolve host and reach SSH port",
  "Authenticate SSH and identify the system",
  "Start live monitoring",
];
type CheckState = "pending" | "running" | "pass" | "fail";

export function WelcomePage() {
  const navigate = useNavigate();
  const {
    servers,
    loading,
    error: serverError,
    selectedServerId,
    setSelectedServerId,
  } = useServersContext();
  const { mask } = usePrivacy();
  const [savedStep, setStep] = useRememberedState("wizard-step", 0);
  const step = Math.max(0, Math.min(5, savedStep));
  const server = servers.find((s) => s.id === selectedServerId);
  const [addOpen, setAddOpen] = useState(false);
  const [editOpen, setEditOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [checks, setChecks] = useState<SetupCheck[] | null>(null);
  const [connection, setConnection] = useState<CheckState[]>(
    CONNECTION_CHECKS.map(() => "pending"),
  );
  const [apps, setApps] = useState<TracelynApp[] | null>(null);
  const [picked, setPicked] = useState<string[]>([]);
  const [connected, setConnected] = useState(false);
  const [appsSaved, setAppsSaved] = useState(false);
  const generation = useRef(0);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    generation.current++;
    setConnection(CONNECTION_CHECKS.map(() => "pending"));
    setChecks(null);
    setApps(null);
    setConnected(false);
    setError(null);
    setBusy(false);
    setAppsSaved(false);
  }, [selectedServerId]);
  useEffect(
    () => () => {
      generation.current++;
    },
    [],
  );
  useEffect(() => {
    heading.current?.focus();
  }, [step]);
  useEffect(() => {
    if (!loading && !server && step > 1) setStep(1);
  }, [loading, server, step, setStep]);

  const finish = () => {
    writePreference("onboarding-dismissed", true);
    setStep(0);
    navigate(servers.length >= 2 ? "/servers" : "/", { replace: true });
  };
  const run = async (job: (token: number) => Promise<void>) => {
    const token = ++generation.current;
    setError(null);
    setBusy(true);
    try {
      await job(token);
    } catch (err) {
      if (generation.current === token) setError(String(err));
    } finally {
      if (generation.current === token) setBusy(false);
    }
  };
  const test = () =>
    run(async (token) => {
      if (!server) throw new Error("Choose or add a server first");
      setConnected(false);
      setConnection(CONNECTION_CHECKS.map(() => "pending"));
      const jobs = [
        async () => {
          if (
            !server.host ||
            !server.username ||
            server.port < 1 ||
            server.port > 65535
          )
            throw new Error(
              "Edit this profile to add a host, username, and valid port",
            );
          if (server.authType === "key" && !server.keyPath)
            throw new Error(
              "Edit this server and choose a private key before connecting",
            );
        },
        () => invoke("probe_server_port", { serverId: server.id }),
        () => invoke("test_onboarding_connection", { serverId: server.id }),
        () => invoke("start_onboarding_monitoring", { serverId: server.id }),
      ];
      for (const [index, job] of jobs.entries()) {
        if (generation.current !== token) return;
        setConnection((previous) =>
          previous.map((value, i) => (i === index ? "running" : value)),
        );
        try {
          await job();
        } catch (err) {
          if (generation.current === token)
            setConnection((previous) =>
              previous.map((value, i) => (i === index ? "fail" : value)),
            );
          throw err;
        }
        if (generation.current !== token) return;
        setConnection((previous) =>
          previous.map((value, i) => (i === index ? "pass" : value)),
        );
      }
      setConnected(true);
    });
  const setup = () =>
    run(async (token) => {
      if (!server) throw new Error("Choose a server first");
      const next = await invoke<SetupCheck[]>("run_onboarding_setup_checks", {
        serverId: server.id,
      });
      if (generation.current === token) setChecks(next);
    });
  const discover = () =>
    run(async (token) => {
      if (!server) throw new Error("Choose a server first");
      const next = await invoke<TracelynApp[]>("discover_onboarding_apps", {
        serverId: server.id,
      });
      if (generation.current !== token) return;
      setApps(next);
      setPicked(next.filter((app) => !app.hidden).map((app) => app.id));
      setAppsSaved(false);
    });
  const saveApps = () =>
    run(async (token) => {
      if (!server || !apps) return;
      await invoke("save_onboarding_app_selection", {
        serverId: server.id,
        appIds: apps.map((app) => app.id),
        selectedIds: picked,
      });
      if (generation.current === token) {
        setAppsSaved(true);
        setStep(5);
      }
    });
  const next = () => {
    setError(null);
    setStep(step + 1);
  };

  if (loading) return <PageSkeleton />;
  return (
    <div className="mx-auto flex w-full max-w-4xl flex-col gap-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-sm font-medium text-muted-foreground">
          Getting started with Tracelyn
        </p>
        <Button variant="ghost" onClick={finish}>
          {busy ? "Continue later" : "Skip for now"}
        </Button>
      </div>
      <ol
        aria-label="Setup progress"
        className="grid grid-cols-3 gap-2 sm:grid-cols-6"
      >
        {STEPS.map((label, i) => (
          <li
            key={label}
            aria-current={i === step ? "step" : undefined}
            className={cn(
              "rounded-lg border border-border px-3 py-2 text-xs",
              i === step && "border-primary bg-accent font-semibold",
              i < step && "bg-muted/40",
            )}
          >
            <span className="mr-2 inline-block" aria-hidden>
              {i < step ? "✓" : i + 1}
            </span>
            {label}
          </li>
        ))}
      </ol>
      <Card>
        <CardHeader>
          <CardTitle>
            <h2 ref={heading} tabIndex={-1} className="text-xl outline-none">
              {step === 0 ? "Your servers, in one clear view" : STEPS[step]}
            </h2>
          </CardTitle>
          <CardDescription>
            {
              [
                "Connect your Linux VPS over SSH, check access, and choose the applications you want to see. This takes a few minutes.",
                "Add a connection profile or import several from SSH config or CSV. You can choose which server to set up first.",
                "Each check runs against your saved profile. Fix a failed step and retry when you are ready.",
                "Run the same read-only checks used by the Setup Guide. Warnings include commands you can review and run yourself.",
                "Discover applications visible to your SSH user. Select the apps to show in Tracelyn. Server CPU, memory, and disk are always included.",
                "Your dashboard is ready. You can revisit this wizard from Settings at any time.",
              ][step]
            }
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-5">
          {(error || serverError) && (
            <Notice tone="critical" title="This step needs attention">
              <Masked text={error ?? serverError ?? ""} />
            </Notice>
          )}
          {step === 0 && (
            <div className="grid gap-4 sm:grid-cols-3">
              {[
                [
                  Server,
                  "Bring your servers",
                  "Use SSH keys or a password stored in Windows Credential Manager.",
                ],
                [
                  Check,
                  "Check access",
                  "See what is available and how to enable missing data.",
                ],
                [
                  Sparkles,
                  "Choose your apps",
                  "Start with automatic discovery, then adjust as your stack grows.",
                ],
              ].map(([Icon, title, description]) => {
                const Illustration = Icon as typeof Server;
                return (
                  <div
                    key={String(title)}
                    className="rounded-xl border border-border bg-muted/30 p-4"
                  >
                    <Illustration
                      className="mb-3 size-5 text-primary"
                      aria-hidden
                    />
                    <h3 className="font-medium">{String(title)}</h3>
                    <p className="mt-2 text-sm text-muted-foreground">
                      {String(description)}
                    </p>
                  </div>
                );
              })}
            </div>
          )}
          {step === 1 && (
            <>
              <div className="flex flex-wrap gap-2">
                <Button onClick={() => setAddOpen(true)}>Add a server</Button>
                <Button variant="outline" onClick={() => setImportOpen(true)}>
                  Import servers
                </Button>
              </div>
              {servers.length > 0 && (
                <div className="flex flex-col gap-2">
                  <Label htmlFor="wizard-server">Set up this server</Label>
                  <SimpleSelect
                    id="wizard-server"
                    value={selectedServerId ?? ""}
                    onValueChange={setSelectedServerId}
                    options={servers.map((s) => ({
                      value: s.id,
                      label: s.name,
                    }))}
                  />
                  <p className="text-sm text-muted-foreground">
                    {servers.length} saved profile
                    {servers.length === 1 ? "" : "s"}. Other servers can be
                    connected from All servers.
                  </p>
                </div>
              )}
            </>
          )}
          {step >= 2 && step <= 4 && (
            <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border bg-muted/30 p-3">
              <p className="text-sm">
                {server ? (
                  <>
                    Setting up <strong>{server.name}</strong>
                  </>
                ) : (
                  "No server selected"
                )}
              </p>
              <Button
                size="sm"
                variant="outline"
                disabled={busy || !server}
                onClick={() => setEditOpen(true)}
              >
                Edit connection
              </Button>
            </div>
          )}
          {step === 2 && (
            <>
              <ol
                className="flex flex-col gap-3"
                aria-label="Live connection checks"
              >
                {CONNECTION_CHECKS.map((label, i) => {
                  const state = connection[i],
                    Icon =
                      state === "running"
                        ? Loader2
                        : state === "pass"
                          ? Check
                          : state === "fail"
                            ? CircleAlert
                            : Circle;
                  return (
                    <li
                      key={label}
                      className="flex items-center gap-3 rounded-lg border border-border p-3"
                    >
                      <Icon
                        aria-hidden
                        className={cn(
                          "size-5 shrink-0",
                          state === "running" && "animate-spin",
                          state === "pass" && "text-status-ok",
                          state === "fail" && "text-status-critical",
                        )}
                      />
                      <span className="flex-1 text-sm">{label}</span>
                      <span className="text-xs text-muted-foreground">
                        {
                          {
                            pending: "Waiting",
                            running: "Checking…",
                            pass: "Passed",
                            fail: "Failed",
                          }[state]
                        }
                      </span>
                    </li>
                  );
                })}
              </ol>
              <Button
                className="w-fit"
                disabled={busy || !server}
                onClick={() => void test()}
              >
                {busy
                  ? "Checking connection…"
                  : connected
                    ? "Run again"
                    : "Run connection test"}
              </Button>
            </>
          )}
          {step === 3 && (
            <>
              <Button
                className="w-fit"
                disabled={busy || !server}
                onClick={() => void setup()}
              >
                {busy
                  ? "Running read-only checks…"
                  : checks
                    ? "Run checks again"
                    : "Run Setup Guide checks"}
              </Button>
              {busy && !checks && <PageSkeleton rows={3} />}
              {checks && (
                <>
                  <p role="status" className="text-sm text-muted-foreground">
                    {checks.filter((c) => c.status === "pass").length} passed ·{" "}
                    {checks.filter((c) => c.status === "warning").length}{" "}
                    warnings ·{" "}
                    {checks.filter((c) => c.status === "fail").length} failed.
                    You can continue and fix missing access later.
                  </p>
                  <ul className="flex flex-col gap-2">
                    {checks.map((check) => (
                      <li key={check.id}>
                        <details className="rounded-lg border border-border p-3">
                          <summary className="cursor-pointer text-sm font-medium">
                            {check.title}
                            <span className="ml-2 text-xs text-muted-foreground">
                              {check.status === "pass"
                                ? "Passed"
                                : check.status === "warning"
                                  ? "Warning"
                                  : check.status === "fail"
                                    ? "Failed"
                                    : "Not checked"}
                            </span>
                          </summary>
                          <p className="mt-3 text-sm text-muted-foreground">
                            {check.explanation}
                          </p>
                          {check.details.map((line, i) => (
                            <p
                              key={i}
                              className="mt-2 break-all font-mono text-xs"
                            >
                              <Masked text={line} />
                            </p>
                          ))}
                          {check.command && (
                            <div className="mt-3 flex flex-col gap-2">
                              <pre className="overflow-auto rounded-md bg-muted p-3 text-xs">
                                <Masked text={check.command} />
                              </pre>
                              <Button
                                className="w-fit"
                                size="sm"
                                variant="outline"
                                onClick={() => {
                                  void navigator.clipboard
                                    .writeText(mask(check.command ?? ""))
                                    .then(() => toast.success("Command copied"))
                                    .catch(() =>
                                      toast.error("Could not copy command"),
                                    );
                                }}
                              >
                                Copy command
                              </Button>
                            </div>
                          )}
                        </details>
                      </li>
                    ))}
                  </ul>
                </>
              )}
            </>
          )}
          {step === 4 && (
            <>
              <Button
                className="w-fit"
                disabled={busy || !server}
                onClick={() => void discover()}
              >
                {busy
                  ? "Discovering applications…"
                  : apps
                    ? "Discover again"
                    : "Discover applications"}
              </Button>
              {busy && !apps && <PageSkeleton rows={3} />}
              {apps && (
                <>
                  <p className="text-sm text-muted-foreground" role="status">
                    {apps.length} discovered · {picked.length} selected.
                    Unselected apps are hidden from application views. Discovery
                    still runs to find new apps.
                  </p>
                  {apps.length === 0 ? (
                    <Notice title="No applications found">
                      You can start with server metrics. Connect as an SSH user
                      with access to your process manager, or add an application
                      manually later.
                    </Notice>
                  ) : (
                    <>
                      <div className="flex gap-2">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => setPicked(apps.map((a) => a.id))}
                        >
                          Select all
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => setPicked([])}
                        >
                          Clear selection
                        </Button>
                      </div>
                      <ul className="max-h-80 space-y-2 overflow-auto">
                        {apps.map((app) => (
                          <li key={app.id}>
                            <label className="flex cursor-pointer items-center gap-3 rounded-lg border border-border p-3">
                              <input
                                type="checkbox"
                                className="size-4 accent-primary"
                                checked={picked.includes(app.id)}
                                onChange={(event) =>
                                  setPicked((previous) =>
                                    event.target.checked
                                      ? [...previous, app.id]
                                      : previous.filter((id) => id !== app.id),
                                  )
                                }
                              />
                              <span className="min-w-0 flex-1">
                                <span className="block truncate font-medium">
                                  <Masked text={app.name} />
                                </span>
                                <span className="text-xs text-muted-foreground">
                                  {app.kind} · {app.status}
                                  {app.ports.length
                                    ? ` · Port ${app.ports.join(", ")}`
                                    : ""}
                                </span>
                              </span>
                            </label>
                          </li>
                        ))}
                      </ul>
                    </>
                  )}
                </>
              )}
            </>
          )}
          {step === 5 && (
            <div className="flex flex-col items-start gap-4 rounded-xl border border-border bg-muted/30 p-6">
              <Check className="size-8 text-status-ok" aria-hidden />
              <h3 className="text-lg font-semibold">Ready to explore</h3>
              <p className="text-sm text-muted-foreground">
                {server?.name ?? "Your servers"}
                {connected
                  ? " is connected"
                  : " can be connected from the dashboard"}
                .
                {appsSaved
                  ? ` ${picked.length} application${picked.length === 1 ? "" : "s"} selected.`
                  : " Monitoring preferences can be adjusted in Applications."}
              </p>
              {checks?.some(
                (c) => c.status === "fail" || c.status === "warning",
              ) && (
                <p className="text-sm text-muted-foreground">
                  Some checks need attention. The Setup Guide keeps the next
                  steps close at hand.
                </p>
              )}
              <Link className={buttonVariants({ variant: "outline" })} to="/setup">
                Open Setup Guide
              </Link>
            </div>
          )}
          <div role="status" aria-live="polite" className="sr-only">
            {busy
              ? "Working on the current step"
              : step === 2
                ? connection
                    .map((state, i) => `${CONNECTION_CHECKS[i]}: ${state}`)
                    .join(". ")
                : ""}
          </div>
          <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border pt-4">
            <Button
              variant="ghost"
              disabled={busy || step === 0}
              onClick={() => {
                setError(null);
                setStep(step - 1);
              }}
            >
              Back
            </Button>
            <div className="flex flex-wrap gap-2">
              {step >= 2 && step <= 4 && (
                <Button variant="ghost" disabled={busy} onClick={next}>
                  Skip this step
                </Button>
              )}
              <Button
                disabled={
                  busy ||
                  (step === 1 && !server) ||
                  (step === 2 && !connected) ||
                  (step === 3 && !checks) ||
                  (step === 4 && !apps)
                }
                onClick={() => {
                  if (step === 5) finish();
                  else if (step === 4) void saveApps();
                  else next();
                }}
              >
                {step === 0
                  ? "Get started"
                  : step === 5
                    ? "Go to dashboard"
                    : step === 4
                      ? "Save selection"
                      : "Continue"}
                <ArrowRight aria-hidden />
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>
      <ServerFormDialog open={addOpen} onOpenChange={setAddOpen} />
      <ServerFormDialog
        open={editOpen}
        onOpenChange={setEditOpen}
        server={server}
        onSaved={() => {
          generation.current++;
          setConnected(false);
          setConnection(CONNECTION_CHECKS.map(() => "pending"));
          setChecks(null);
          setApps(null);
          setAppsSaved(false);
          setError(null);
          setStep(2);
        }}
      />
      <ServerImportDialog
        open={importOpen}
        onOpenChange={setImportOpen}
        onImported={(imported) => {
          if (imported[0]) setSelectedServerId(imported[0].id);
        }}
      />
    </div>
  );
}
