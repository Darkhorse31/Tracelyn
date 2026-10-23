import { invoke } from "@tauri-apps/api/core";
import type { AlertEventRecord, AlertRule } from "@/lib/alerts-types";

export async function listAlertRules(): Promise<AlertRule[]> {
  return invoke<AlertRule[]>("list_alert_rules");
}

export async function upsertAlertRule(rule: AlertRule): Promise<AlertRule> {
  return invoke<AlertRule>("upsert_alert_rule", { rule });
}

export async function deleteAlertRule(id: string): Promise<void> {
  return invoke("delete_alert_rule", { id });
}

export async function listAlertHistory(
  serverId?: string,
  limit = 100,
): Promise<AlertEventRecord[]> {
  return invoke<AlertEventRecord[]>("list_alert_history", { serverId, limit });
}
