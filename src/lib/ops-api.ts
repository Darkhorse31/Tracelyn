import { invoke } from "@tauri-apps/api/core";
import type {
  ActionOutcome,
  ActionPreview,
  ActionRequest,
  AuditFilter,
  AuditRow,
} from "@/lib/ops-types";

export function opsGetMode(): Promise<boolean> {
  return invoke<boolean>("ops_get_mode");
}

export function opsSetMode(enabled: boolean): Promise<void> {
  return invoke("ops_set_mode", { enabled });
}

export function opsSetSudoPassword(serverId: string, password: string): Promise<void> {
  return invoke("ops_set_sudo_password", { serverId, password });
}

export function opsSudoConfigured(serverId: string): Promise<{ configured: boolean }> {
  return invoke("ops_sudo_configured", { serverId });
}

export function opsPreview(request: ActionRequest): Promise<ActionPreview> {
  return invoke("ops_preview", { request });
}

export function opsRun(request: ActionRequest): Promise<ActionOutcome> {
  return invoke("ops_run", { request });
}

export function opsListAudit(filter: AuditFilter): Promise<AuditRow[]> {
  return invoke("ops_list_audit", { filter });
}

export function opsExportAudit(filter: AuditFilter): Promise<string> {
  return invoke("ops_export_audit", { filter });
}

export function opsTerminalOpen(
  serverId: string,
  cols: number,
  rows: number,
): Promise<{ tabId: string }> {
  return invoke("ops_terminal_open", { request: { serverId, cols, rows } });
}

export function opsTerminalWrite(tabId: string, data: string): Promise<void> {
  return invoke("ops_terminal_write", { request: { tabId, data } });
}

export function opsTerminalResize(tabId: string, cols: number, rows: number): Promise<void> {
  return invoke("ops_terminal_resize", { request: { tabId, cols, rows } });
}

export function opsTerminalClose(tabId: string): Promise<void> {
  return invoke("ops_terminal_close", { request: { tabId } });
}
