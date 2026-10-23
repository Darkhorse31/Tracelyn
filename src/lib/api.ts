import { invoke } from "@tauri-apps/api/core";
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

export async function subscribeServerMetrics(serverId: string): Promise<void> {
  return invoke("subscribe_server_metrics", { serverId });
}

export async function unsubscribeServerMetrics(serverId: string): Promise<void> {
  return invoke("unsubscribe_server_metrics", { serverId });
}
