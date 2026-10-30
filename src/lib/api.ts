import { invoke } from "@tauri-apps/api/core";
import type { DiagnosticEntry, TracelynApp } from "@/lib/apps-types";
import type {
  ErrorIssue,
  InsightSetup,
  ServiceMap,
  SlowQueryRow,
  TraceDetail,
  TraceQuery,
  TraceSummary,
} from "@/lib/insight-types";
import type { LogLine, LogStreamSnapshot } from "@/lib/log-types";
import type { RouteRule } from "@/lib/apm-types";
import type { ApmData, EndpointRow } from "@/lib/apm-types";
import type { LiveLogLine, WebTrafficMetrics } from "@/lib/web-traffic-types";
import type {
  ConnectionTestResult,
  ServerRecord,
  ServerStatus,
  TestConnectionInput,
  UpsertServerInput,
} from "@/lib/types";

export async function listServers(): Promise<ServerRecord[]> {
  return invoke<ServerRecord[]>("list_servers");
}

export async function listServerStatuses(): Promise<ServerStatus[]> {
  return invoke<ServerStatus[]>("list_server_statuses");
}

export async function upsertServer(input: UpsertServerInput): Promise<ServerRecord> {
  return invoke<ServerRecord>("upsert_server", { input });
}

export async function deleteServer(id: string): Promise<void> {
  return invoke("delete_server", { id });
}

export async function trustHostKey(host: string, port: number, fingerprint: string): Promise<void> {
  return invoke("trust_host_key", { host, port, fingerprint });
}

export async function readCrashLog(): Promise<string> {
  return invoke("read_crash_log");
}

export async function openCrashLog(): Promise<void> {
  return invoke("open_crash_log");
}

export async function testServerConnection(
  input: TestConnectionInput,
): Promise<ConnectionTestResult> {
  return invoke<ConnectionTestResult>("test_server_connection", { input });
}

export async function connectServer(id: string): Promise<ServerStatus> {
  return invoke<ServerStatus>("connect_server", { id });
}

export async function disconnectServer(id: string): Promise<ServerStatus> {
  return invoke<ServerStatus>("disconnect_server", { id });
}

export async function subscribeOverviewMetrics(): Promise<void> {
  return invoke("subscribe_overview_metrics");
}

export async function unsubscribeOverviewMetrics(): Promise<void> {
  return invoke("unsubscribe_overview_metrics");
}

export async function setLiveMetrics(serverId: string, enabled: boolean): Promise<void> {
  return invoke("set_live_metrics", { serverId, enabled });
}

export async function subscribeProcesses(serverId: string): Promise<void> {
  return invoke("subscribe_processes", { serverId });
}

export async function unsubscribeProcesses(serverId: string): Promise<void> {
  return invoke("unsubscribe_processes", { serverId });
}

export async function subscribeServerMetrics(serverId: string): Promise<void> {
  return invoke("subscribe_server_metrics", { serverId });
}

export async function unsubscribeServerMetrics(serverId: string): Promise<void> {
  return invoke("unsubscribe_server_metrics", { serverId });
}

export async function subscribeWebTraffic(serverId: string): Promise<void> {
  return invoke("subscribe_web_traffic", { serverId });
}

export async function unsubscribeWebTraffic(serverId: string): Promise<void> {
  return invoke("unsubscribe_web_traffic", { serverId });
}

export async function subscribeApmMetrics(serverId: string): Promise<void> {
  return invoke("subscribe_apm_metrics", { serverId });
}

export async function unsubscribeApmMetrics(serverId: string): Promise<void> {
  return invoke("unsubscribe_apm_metrics", { serverId });
}

export async function subscribeRedisMetrics(serverId: string): Promise<void> {
  return invoke("subscribe_redis_metrics", { serverId });
}

export async function unsubscribeRedisMetrics(serverId: string): Promise<void> {
  return invoke("unsubscribe_redis_metrics", { serverId });
}

export async function getRouteRules(serverId: string): Promise<RouteRule[]> {
  return invoke<RouteRule[]>("get_route_rules", { serverId });
}

export async function setRouteRules(
  serverId: string,
  rules: RouteRule[],
): Promise<RouteRule[]> {
  return invoke<RouteRule[]>("set_route_rules", { serverId, rules });
}

export async function listApps(serverId: string): Promise<TracelynApp[]> {
  return invoke<TracelynApp[]>("list_apps", { serverId });
}

export async function upsertManualApp(app: TracelynApp): Promise<TracelynApp> {
  return invoke<TracelynApp>("upsert_manual_app_cmd", { app });
}

export async function hideApp(
  serverId: string,
  appId: string,
  hidden: boolean,
): Promise<void> {
  return invoke("hide_app", { serverId, appId, hidden });
}

export async function mergeApps(
  serverId: string,
  primaryId: string,
  duplicateIds: string[],
): Promise<TracelynApp> {
  return invoke<TracelynApp>("merge_apps", {
    input: { serverId, primaryId, duplicateIds },
  });
}

export async function rediscoverApps(serverId: string): Promise<TracelynApp[]> {
  return invoke<TracelynApp[]>("rediscover_apps", { serverId });
}

export async function subscribeApps(serverId: string): Promise<void> {
  return invoke("subscribe_apps", { serverId });
}

export async function unsubscribeApps(serverId: string): Promise<void> {
  return invoke("unsubscribe_apps", { serverId });
}

export async function subscribeLogStream(
  serverId: string,
  appIds?: string[],
  sources?: string[],
): Promise<void> {
  return invoke("subscribe_log_stream", { serverId, appIds, sources });
}

export async function unsubscribeLogStream(serverId: string): Promise<void> {
  return invoke("unsubscribe_log_stream", { serverId });
}

export async function getLogSnapshot(
  serverId: string,
): Promise<LogStreamSnapshot | null> {
  return invoke<LogStreamSnapshot | null>("get_log_snapshot", { serverId });
}

export async function setLogPaused(serverId: string, paused: boolean): Promise<void> {
  return invoke("set_log_paused", { serverId, paused });
}

export async function getServerDiagnostics(serverId: string): Promise<DiagnosticEntry[]> {
  return invoke<DiagnosticEntry[]>("get_server_diagnostics", { serverId });
}

export async function getRedisSnapshot(serverId: string): Promise<unknown | null> {
  return invoke("get_redis_snapshot", { serverId });
}

export async function setRedisUsername(serverId: string, username: string): Promise<void> {
  return invoke("set_redis_username", { serverId, username });
}

export interface RedisTargetConfig {
  port?: number | null;
  socket?: string | null;
  dockerContainer?: string | null;
  label?: string | null;
}

export async function getRedisTarget(serverId: string): Promise<RedisTargetConfig> {
  return invoke<RedisTargetConfig>("get_redis_target", { serverId });
}

export async function testRedisConnection(
  serverId: string,
  target: RedisTargetConfig,
): Promise<string> {
  return invoke<string>("test_redis_connection", { serverId, target });
}

export async function setRedisTarget(
  serverId: string,
  target: RedisTargetConfig,
): Promise<void> {
  return invoke("set_redis_target", { serverId, target });
}

export interface WebTrafficSnapshot {
  serverId: string;
  metrics: WebTrafficMetrics;
  logs: LiveLogLine[];
  permissionDenied: boolean;
  webDetected: boolean;
}

export async function getWebTrafficSnapshot(
  serverId: string,
): Promise<WebTrafficSnapshot | null> {
  return invoke<WebTrafficSnapshot | null>("get_web_traffic_snapshot", { serverId });
}

export interface ApmSnapshot {
  serverId: string;
  endpoints: EndpointRow[];
  heatmap: ApmData["heatmap"];
  timingNotice?: { message: string; suggestedLine: string };
}

export async function getApmSnapshot(serverId: string): Promise<ApmSnapshot | null> {
  return invoke<ApmSnapshot | null>("get_apm_snapshot", { serverId });
}

export async function listTraces(query: TraceQuery): Promise<TraceSummary[]> {
  return invoke<TraceSummary[]>("list_traces", { query });
}

export async function getTrace(serverId: string, traceId: string): Promise<TraceDetail> {
  return invoke<TraceDetail>("get_trace", { serverId, traceId });
}

export async function getServiceMap(
  serverId: string,
  startMs: number,
  endMs: number,
): Promise<ServiceMap> {
  return invoke<ServiceMap>("get_service_map", { serverId, startMs, endMs });
}

export async function listErrorIssues(serverId: string, appId?: string): Promise<ErrorIssue[]> {
  return invoke<ErrorIssue[]>("list_error_issues", { serverId, appId });
}

export async function setErrorIssueState(
  serverId: string,
  fingerprint: string,
  issueState: string,
): Promise<void> {
  return invoke("set_error_issue_state", { serverId, fingerprint, issueState });
}

export async function listSlowQueries(serverId: string, engine?: string): Promise<SlowQueryRow[]> {
  return invoke<SlowQueryRow[]>("list_slow_queries", { serverId, engine });
}

export async function getInsightSetup(serverId: string): Promise<InsightSetup> {
  return invoke<InsightSetup>("get_insight_setup", { serverId });
}

export async function setOtelPaths(serverId: string, appId: string, paths: string[]): Promise<void> {
  return invoke("set_otel_paths", { serverId, appId, paths });
}

export async function setSlowlogPath(serverId: string, engine: string, path: string): Promise<void> {
  return invoke("set_slowlog_path", { serverId, engine, path });
}

export interface SetupCheck {
  id: string;
  title: string;
  status: "pass" | "warning" | "fail" | "pending" | string;
  explanation: string;
  command?: string | null;
  details: string[];
}

export async function runSetupChecks(serverId: string): Promise<SetupCheck[]> {
  return invoke<SetupCheck[]>("run_setup_checks", { serverId });
}

export interface MonitoringStep {
  id: string;
  title: string;
  status: "waiting" | "running" | "done" | "skipped" | "failed" | string;
  detail: string;
  reason: string;
  details: string;
  script: string;
}

export interface MonitoringSetup {
  steps: MonitoringStep[];
  pause: "none" | "needPassword" | "chooseApps" | string;
  summary: string;
  detected: { nodes: { name: string; cwd: string }[] };
  selectedApps: string[];
}

export interface MonitoringEvent {
  serverId: string;
  wizard: MonitoringSetup;
}

export async function monitoringSetupStatus(serverId: string): Promise<MonitoringSetup> {
  return invoke<MonitoringSetup>("monitoring_setup_status", { serverId });
}

export async function monitoringSetupStart(
  serverId: string,
  sudoPassword?: string,
  apps?: string[],
): Promise<MonitoringSetup> {
  return invoke<MonitoringSetup>("monitoring_setup_start", { serverId, sudoPassword, apps });
}

export async function monitoringSetupRetry(serverId: string): Promise<MonitoringSetup> {
  return invoke<MonitoringSetup>("monitoring_setup_retry", { serverId });
}

export async function monitoringSetupUndo(serverId: string): Promise<MonitoringSetup> {
  return invoke<MonitoringSetup>("monitoring_setup_undo", { serverId });
}

export interface SshKeyList {
  directory: string;
  keys: string[];
}

export interface KeyInspection {
  ok: boolean;
  message: string;
  putty: boolean;
  encrypted: boolean;
  needsPassphrase: boolean;
}

export async function listSshPrivateKeys(): Promise<SshKeyList> {
  return invoke<SshKeyList>("list_ssh_private_keys");
}

export async function inspectSshPrivateKey(
  path: string,
  passphrase?: string,
): Promise<KeyInspection> {
  return invoke<KeyInspection>("inspect_ssh_private_key", {
    path,
    passphrase: passphrase ?? null,
  });
}

export async function logsForTrace(serverId: string, traceId: string, window: number): Promise<LogLine[]> {
  return invoke<LogLine[]>("logs_for_trace", { serverId, traceId, window });
}

export async function searchServerLogs(serverId: string, pattern: string): Promise<string[]> {
  return invoke<string[]>("search_server_logs", { serverId, pattern });
}

export interface SavedSearch {
  name: string;
  query: string;
}

export async function listSavedSearches(serverId: string): Promise<SavedSearch[]> {
  return invoke<SavedSearch[]>("list_saved_searches", { serverId });
}

export async function saveSavedSearch(serverId: string, name: string, query: string): Promise<void> {
  return invoke("save_saved_search", { serverId, name, query });
}

export async function deleteSavedSearch(serverId: string, name: string): Promise<void> {
  return invoke("delete_saved_search", { serverId, name });
}

export interface MetricSeriesPoint {
  tsMs: number;
  value: number;
}

export interface MetricsAround {
  cpu: { points: MetricSeriesPoint[] };
  memory: { points: MetricSeriesPoint[] };
  load: { points: MetricSeriesPoint[] };
}

export async function metricsAround(serverId: string, timestampMs: number, windowMs: number): Promise<MetricsAround> {
  return invoke<MetricsAround>("metrics_around", { serverId, timestampMs, windowMs });
}
