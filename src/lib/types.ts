export type AuthType = "key" | "password";

export type ConnectionStatus =
  | "disconnected"
  | "connecting"
  | "connected"
  | "degraded"
  | "offline";

export interface ServerRecord {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authType: AuthType;
  keyPath?: string | null;
  labels: string[];
}

export interface ServerStatus {
  id: string;
  status: ConnectionStatus;
  osInfo?: string | null;
  hostname?: string | null;
  lastError?: string | null;
}

export interface ConnectionTestResult {
  success: boolean;
  osInfo: string;
  hostname?: string | null;
}

export interface UpsertServerInput {
  id?: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authType: AuthType;
  keyPath?: string;
  labels: string[];
  password?: string;
  keyPassphrase?: string;
}

export interface TestConnectionInput {
  serverId?: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authType: AuthType;
  keyPath?: string;
  password?: string;
  keyPassphrase?: string;
}
