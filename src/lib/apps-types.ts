export type AppKind =
  | "pm2"
  | "systemd"
  | "docker"
  | "nginx"
  | "apache"
  | "service"
  | "manual";

export type AppOrigin = "discovered" | "manual";
export type AppStatus = "online" | "stopped" | "errored" | "unknown";

export type LogSourceKind =
  | "stdout"
  | "stderr"
  | "journald"
  | "docker"
  | "nginx_access"
  | "nginx_error"
  | "apache_access"
  | "custom";

export interface LogSourceRef {
  id: string;
  kind: LogSourceKind;
  path?: string | null;
  containerId?: string | null;
  vhost?: string | null;
  readable: boolean;
  issue?: string | null;
  fixHint?: string | null;
}

export interface AppMetrics {
  reqPerSec?: number | null;
  errorRate?: number | null;
  p95Ms?: number | null;
}

export interface TracelynApp {
  id: string;
  serverId: string;
  name: string;
  kind: AppKind;
  status: AppStatus;
  origin: AppOrigin;
  hidden: boolean;
  pids: number[];
  ports: number[];
  domains: string[];
  tags: string[];
  logSources: LogSourceRef[];
  cpuPercent?: number | null;
  memoryBytes?: number | null;
  restarts?: number | null;
  uptimeSecs?: number | null;
  cwd?: string | null;
  interpreter?: string | null;
  linkedAppIds: string[];
  metrics?: AppMetrics | null;
}

export interface DiagnosticEntry {
  collector: string;
  status: "ok" | "warning" | "error";
  message: string;
  fixHint?: string | null;
}

export const APP_KINDS: AppKind[] = [
  "pm2",
  "systemd",
  "docker",
  "nginx",
  "apache",
  "service",
  "manual",
];
