import { syncCompanionConfig } from "@/lib/companion-sync";
import { invoke } from "@tauri-apps/api/core";
import type { AlertEventRecord, AlertRule, ChartMarker } from "@/lib/alerts-types";

export async function listAlertRules(): Promise<AlertRule[]> {
  return invoke<AlertRule[]>("list_alert_rules");
}

export async function upsertAlertRule(rule: AlertRule): Promise<AlertRule> {
  const result = await invoke<AlertRule>("upsert_alert_rule", { rule });
  await syncCompanionConfig();
  return result;
}

export async function deleteAlertRule(id: string): Promise<void> {
  await invoke("delete_alert_rule", { id });
  await syncCompanionConfig();
}

export async function listAlertHistory(
  serverId?: string,
  limit = 100,
): Promise<AlertEventRecord[]> {
  return invoke<AlertEventRecord[]>("list_alert_history", { serverId, limit });
}

const markerLoads = new Map<string, Promise<ChartMarker[]>>();

export function listChartMarkers(
  serverId: string,
  startMs: number,
  endMs: number,
): Promise<ChartMarker[]> {
  const key = `${serverId}:${startMs}:${endMs}`;
  const pending = markerLoads.get(key);
  if (pending) return pending;
  const load = invoke<ChartMarker[]>("list_chart_markers", { serverId, startMs, endMs }).finally(() => {
    setTimeout(() => markerLoads.delete(key), 4000);
  });
  markerLoads.set(key, load);
  return load;
}

export function silenceAlertRule(id: string): Promise<AlertRule> {
  return invoke<AlertRule>("silence_alert_rule", { id });
}

export function alertTestSend(): Promise<string> {
  return invoke<string>("alert_test_send");
}
