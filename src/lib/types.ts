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

export interface OverviewServerMetrics {
  serverId: string;
  cpuPercent: number;
  memoryPercent: number;
  diskPercent: number;
  load1: number;
  cpuSparkline: number[];
  memorySparkline: number[];
  diskSparkline: number[];
  loadSparkline: number[];
}

export interface MetricPoint {
  t: number;
  v: number;
}

export interface ProcessRow {
  pid: number;
  user: string;
  cpuPercent: number;
  memoryPercent: number;
  command: string;
}

export interface ServiceRow {
  name: string;
  active: boolean;
  state: string;
}

export interface SsSummary {
  tcpTotal?: number | null;
  tcpEstablished?: number | null;
}

export interface ServerMetricsSnapshot {
  serverId: string;
  timestampMs: number;
  cpuPercent: number;
  memoryPercent: number;
  load1: number;
  load5: number;
  load15: number;
  diskPercent: number;
  diskReadBps: number;
  diskWriteBps: number;
  netInBps: number;
  netOutBps: number;
  cpuHistory: MetricPoint[];
  memoryHistory: MetricPoint[];
  loadHistory: MetricPoint[];
  diskUsageHistory: MetricPoint[];
  diskReadHistory: MetricPoint[];
  diskWriteHistory: MetricPoint[];
  netInHistory: MetricPoint[];
  netOutHistory: MetricPoint[];
  processes: ProcessRow[];
  services: ServiceRow[];
  ssSummary: SsSummary;
  collectorError?: string | null;
}
