import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";

export type Phase =
  | "loading"
  | "ok"
  | "partial"
  | "not_installed"
  | "auth_needed"
  | "permission_denied"
  | "timed_out"
  | "error"
  | "installed_stopped"
  | "not_configured";
export interface Resource {
  id: string;
  name: string;
  state: string;
  fields: Record<string, unknown>;
}
export interface Snapshot {
  collector: string;
  phase: Phase;
  message: string;
  fixHint: string | null;
  detail?: string | null;
  sampledAtMs: number;
  rows: Resource[];
}
export interface Target {
  host: string;
  port: number | null;
  container: string;
  database: string;
}
export interface TargetView {
  target: Target;
  hasCredentials: boolean;
}
export interface Check {
  id: string;
  serverId: string;
  url: string;
  intervalSeconds: number;
  enabled: boolean;
  allowSelfSigned?: boolean;
}
export interface CheckResult {
  tsMs: number;
  ok: boolean;
  statusCode: number | null;
  addresses: string[];
  dnsMs: number | null;
  connectMs: number | null;
  tlsMs: number | null;
  ttfbMs: number | null;
  totalMs: number;
  error: string | null;
}
export interface CheckView {
  check: Check;
  history: CheckResult[];
}
export const infraApi = {
  snapshot: (
    serverId: string,
    collectors?: string[],
    includeHistory = false,
    summaryOnly = false,
  ) =>
    invoke<Snapshot[]>("get_infra_snapshot", {
      serverId,
      collectors: collectors ?? null,
      includeHistory,
      summaryOnly,
    }),
  refresh: (serverId: string) => invoke<void>("refresh_infra", { serverId }),
  target: (serverId: string, engine: string) =>
    invoke<TargetView>("get_infra_target", { serverId, engine }),
  saveTarget: (
    serverId: string,
    engine: string,
    target: Target,
    credentials?: { username: string; password: string },
  ) =>
    invoke<void>("set_infra_target", {
      serverId,
      engine,
      target,
      credentials: credentials ?? null,
    }),
  domains: (serverId: string) =>
    invoke<string[]>("list_infra_domains", { serverId }),
  checks: (serverId: string) =>
    invoke<CheckView[]>("list_uptime_checks", { serverId }),
  saveCheck: (check: Check) => invoke<Check>("upsert_uptime_check", { check }),
  deleteCheck: (serverId: string, checkId: string) =>
    invoke<void>("delete_uptime_check", { serverId, checkId }),
  runCheck: (serverId: string, checkId: string) =>
    invoke<CheckResult>("run_uptime_check", { serverId, checkId }),
};

/** Never overlap requests; ignore responses from an old server or an unmounted page. */
export function useInfra(
  serverId: string | null,
  collectorKey = "",
  includeHistory = false,
  summaryOnly = false,
) {
  const [data, setData] = useState<Snapshot[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const reload = useCallback(() => setRevision((n) => n + 1), []);
  useEffect(() => {
    setData(null);
    setError(null);
    if (!serverId) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const next = await infraApi.snapshot(
          serverId,
          collectorKey ? collectorKey.split(",") : undefined,
          includeHistory,
          summaryOnly,
        );
        if (active) {
          setData(next);
          setError(null);
        }
      } catch (err) {
        if (active) setError(String(err));
      } finally {
        if (active) timer = setTimeout(poll, 5000);
      }
    };
    void poll();
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [serverId, revision, collectorKey, includeHistory, summaryOnly]);
  return { data, error, reload };
}
