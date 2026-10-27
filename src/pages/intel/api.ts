import { invoke } from "@tauri-apps/api/core";
import type {
  AnomalyEvent,
  BandPoint,
  Channel,
  ChannelWrite,
  DigestSpec,
  EvalOut,
  FileOut,
  ImportSummary,
  Incident,
  IncidentDetail,
  IntelRule,
  SavedView,
} from "@/pages/intel/types";

export function listIntelRules(): Promise<IntelRule[]> {
  return invoke("intel_list_rules");
}

export function upsertIntelRule(rule: IntelRule): Promise<IntelRule> {
  return invoke("intel_upsert_rule", { rule });
}

export function deleteIntelRule(id: string): Promise<void> {
  return invoke("intel_delete_rule", { id });
}

export function evaluateIntel(serverId: string): Promise<EvalOut> {
  return invoke("intel_evaluate", { serverId });
}

export function listChannels(): Promise<Channel[]> {
  return invoke("intel_list_channels");
}

export function upsertChannel(input: ChannelWrite): Promise<Channel> {
  return invoke("intel_upsert_channel", { input });
}

export function deleteChannel(id: string): Promise<void> {
  return invoke("intel_delete_channel", { id });
}

export function testChannel(id: string): Promise<string> {
  return invoke("intel_test_channel", { id });
}

export function listIncidents(serverId?: string): Promise<Incident[]> {
  return invoke("intel_list_incidents", { serverId: serverId ?? null });
}

export function getIncident(id: string): Promise<IncidentDetail> {
  return invoke("intel_get_incident", { id });
}

export function ackIncident(id: string): Promise<void> {
  return invoke("intel_ack_incident", { id });
}

export function resolveIncident(id: string): Promise<void> {
  return invoke("intel_resolve_incident", { id });
}

export function addIncidentNote(id: string, text: string): Promise<void> {
  return invoke("intel_add_incident_note", { id, text });
}

export function snoozeIncident(id: string, minutes: 15 | 60 | 1440): Promise<void> {
  return invoke("intel_snooze_incident", { id, minutes });
}

export function unreadAlertCount(): Promise<number> {
  return invoke("intel_unread_count");
}

export function markAlertsRead(): Promise<void> {
  return invoke("intel_mark_alerts_read");
}

export function alertSoundEnabled(): Promise<boolean> {
  return invoke("intel_sound_enabled");
}

export function setAlertSound(enabled: boolean): Promise<void> {
  return invoke("intel_set_sound", { enabled });
}

export function anomalySeries(
  serverId: string,
  metricKey: string,
  rangeStartMs: number,
  rangeEndMs: number,
): Promise<BandPoint[]> {
  return invoke("intel_anomaly_series", { serverId, metricKey, rangeStartMs, rangeEndMs });
}

export function listAnomalies(
  serverId: string,
  rangeStartMs: number,
  rangeEndMs: number,
): Promise<AnomalyEvent[]> {
  return invoke("intel_list_anomalies", { serverId, rangeStartMs, rangeEndMs });
}

export function exportReport(query: {
  format: "html" | "pdf";
  incidentId?: string;
  serverId?: string;
  metricKey?: string;
  rangeStartMs?: number;
  rangeEndMs?: number;
}): Promise<FileOut> {
  return invoke("intel_export_report", { query });
}

export function exportConfig(): Promise<FileOut> {
  return invoke("intel_export_config");
}

export function importConfig(raw: string): Promise<ImportSummary> {
  return invoke("intel_import_config", { raw });
}

export function listViews(): Promise<SavedView[]> {
  return invoke("intel_list_views");
}

export function upsertView(view: SavedView): Promise<SavedView> {
  return invoke("intel_upsert_view", { view });
}

export function deleteView(id: string): Promise<void> {
  return invoke("intel_delete_view", { id });
}

export function listDigests(): Promise<DigestSpec[]> {
  return invoke("intel_list_digests");
}

export function upsertDigest(digest: DigestSpec): Promise<DigestSpec> {
  return invoke("intel_upsert_digest", { digest });
}

export function deleteDigest(id: string): Promise<void> {
  return invoke("intel_delete_digest", { id });
}

export function sendDigest(id: string): Promise<string> {
  return invoke("intel_send_digest", { id });
}

export function downloadFile(file: FileOut) {
  const binary = atob(file.base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  const blob = new Blob([bytes], { type: file.mime });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = file.filename;
  link.click();
  URL.revokeObjectURL(url);
}
