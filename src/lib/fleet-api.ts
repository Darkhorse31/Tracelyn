import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type { ImportPreview } from "./server-import";

export interface FleetSummary {
  serverId: string;
  appCount: number;
  activeAlertCount: number;
  lastSeenMs: number | null;
  cpuPercent: number | null;
  memoryPercent: number | null;
  diskPercent: number | null;
}
export function previewServerImport(format: "ssh" | "csv", text?: string) {
  return invoke<ImportPreview>("preview_server_import", { format, text });
}
export function getFleetSummary() {
  return invoke<FleetSummary[]>("get_fleet_summary");
}
export async function exportServers() {
  const path = await save({
    title: "Export server profiles",
    defaultPath: "tracelyn-servers.csv",
    filters: [{ name: "CSV", extensions: ["csv"] }],
  });
  if (!path) return false;
  await invoke("export_server_profiles", { path });
  return true;
}
