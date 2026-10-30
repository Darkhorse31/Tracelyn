export type IntelKind =
  | "metric"
  | "log_pattern"
  | "error_rate"
  | "p95"
  | "ssl_expiry"
  | "uptime"
  | "app_crash"
  | "restart_loop"
  | "app_down"
  | "new_error"
  | "disk_forecast"
  | "server_unreachable"
  | "redis_down"
  | "service_stopped"
  | "ssh_login"
  | "ssh_fail_burst"
  | "deploy";

export type Severity = "info" | "warning" | "critical";

export interface RuleParams {
  threshold: number;
  clearThreshold?: number | null;
  durationSecs: number;
  clearDurationSecs: number;
  metricKey: string;
  pattern: string;
  windowSecs: number;
  host: string;
  port: number;
  cooldownSecs: number;
  quietStartMin?: number | null;
  quietEndMin?: number | null;
  appId: string;
}

export interface IntelRule {
  id: string;
  serverId?: string | null;
  kind: IntelKind;
  name: string;
  enabled: boolean;
  severity: Severity;
  params: RuleParams;
  createdAtMs: number;
  updatedAtMs: number;
  firing?: boolean;
}

export type ChannelKind =
  | "windows"
  | "slack"
  | "discord"
  | "teams"
  | "telegram"
  | "ntfy"
  | "email"
  | "webhook";

export interface ChannelConfig {
  chatId: string;
  smtpHost: string;
  smtpPort: number;
  smtpTls: string;
  mailFrom: string;
  mailTo: string;
  username: string;
}

export interface Channel {
  id: string;
  name: string;
  kind: ChannelKind;
  enabled: boolean;
  config: ChannelConfig;
  hasSecret: boolean;
  createdAtMs: number;
}

export interface ChannelWrite {
  id?: string;
  name: string;
  kind: ChannelKind;
  enabled: boolean;
  config: ChannelConfig;
  secret?: string;
}

export interface Incident {
  id: string;
  serverId: string;
  title: string;
  severity: Severity | string;
  status: "open" | "ack" | "resolved" | string;
  openedAtMs: number;
  updatedAtMs: number;
  ackedAtMs?: number | null;
  resolvedAtMs?: number | null;
  snoozeUntilMs?: number | null;
  unread?: boolean;
}

export interface TimelineEntry {
  id: string;
  tsMs: number;
  kind: string;
  message: string;
}

export interface IncidentDetail {
  incident: Incident;
  timeline: TimelineEntry[];
}

export interface AnomalyEvent {
  id: string;
  serverId: string;
  metricKey: string;
  tsMs: number;
  value: number;
  baseline: number;
  z: number;
  message: string;
}

export interface BandPoint {
  tsMs: number;
  value: number;
  baseline: number;
  low: number;
  high: number;
  z: number;
  anomalous: boolean;
}

export interface SavedView {
  id: string;
  name: string;
  path: string;
  note: string;
}

export interface DigestSpec {
  id: string;
  name: string;
  cadence: "daily" | "weekly";
  hourUtc: number;
  weekday: number;
  channelId: string;
  enabled: boolean;
  lastSentMs?: number | null;
  lastAttemptMs?: number | null;
  lastError?: string | null;
}

export interface EvalOut {
  fired: number;
  cleared: number;
  errors: string[];
}

export interface FileOut {
  filename: string;
  mime: string;
  base64: string;
}

export interface ImportSummary {
  servers: number;
  alertRules: number;
  intelRules: number;
  channels: number;
  savedViews: number;
  digests: number;
}

export const KIND_LABELS: Record<IntelKind, string> = {
  metric: "Metric threshold",
  log_pattern: "Log pattern rate",
  error_rate: "Error rate",
  p95: "p95 latency",
  ssl_expiry: "SSL expiry",
  uptime: "Uptime check",
  app_crash: "App crashed or exited",
  restart_loop: "Restart loop",
  app_down: "App down",
  new_error: "New error type",
  disk_forecast: "Disk filling up",
  server_unreachable: "Server unreachable",
  redis_down: "Redis down",
  service_stopped: "Service stopped",
  ssh_login: "New SSH login",
  ssh_fail_burst: "Failed SSH login burst",
  deploy: "Deploy detected",
};

export const CHANNEL_LABELS: Record<ChannelKind, string> = {
  windows: "Windows notification",
  slack: "Slack",
  discord: "Discord",
  teams: "Microsoft Teams",
  telegram: "Telegram",
  ntfy: "ntfy",
  email: "Email (SMTP)",
  webhook: "Webhook",
};

export const METRIC_KEYS = [
  "cpu.percent",
  "memory.percent",
  "disk.percent",
  "load.1",
  "web.req_per_sec",
  "http.5xx_rate_percent",
  "endpoint.p95_ms",
  "app.error_rate",
  "app.p95_ms",
] as const;

const LOW_KINDS: IntelKind[] = [
  "ssl_expiry",
  "uptime",
  "app_down",
  "disk_forecast",
  "server_unreachable",
  "redis_down",
  "service_stopped",
];

export function emptyParams(kind: IntelKind): RuleParams {
  const low = LOW_KINDS.includes(kind);
  const threshold =
    kind === "ssl_expiry" ? 14
    : kind === "disk_forecast" ? 7
    : kind === "restart_loop" ? 5
    : kind === "ssh_fail_burst" ? 10
    : kind === "p95" ? 500
    : low ? 0
    : kind === "app_crash" || kind === "new_error" || kind === "ssh_login" || kind === "deploy" ? 1
    : 90;
  return {
    threshold,
    clearThreshold: low ? (kind === "ssl_expiry" ? 21 : kind === "disk_forecast" ? 14 : 0.5) : undefined,
    durationSecs: kind === "metric" || kind === "error_rate" || kind === "p95" ? 300 : kind === "app_down" || kind === "server_unreachable" ? 60 : 0,
    clearDurationSecs: 30,
    metricKey:
      kind === "error_rate" ? "http.5xx_rate_percent"
      : kind === "p95" ? "endpoint.p95_ms"
      : kind === "disk_forecast" ? "disk.forecast_days"
      : kind === "server_unreachable" ? "server.connected"
      : kind === "redis_down" ? "redis.up"
      : kind === "service_stopped" ? "service.nginx"
      : kind === "metric" ? "cpu.percent"
      : "",
    pattern: kind === "service_stopped" ? "nginx" : "",
    windowSecs: kind === "restart_loop" ? 600 : 60,
    host: "",
    port: kind === "ssl_expiry" ? 443 : kind === "uptime" || kind === "app_down" ? 80 : 0,
    cooldownSecs: kind === "app_crash" || kind === "restart_loop" || kind === "deploy" ? 900 : 300,
    quietStartMin: null,
    quietEndMin: null,
    appId: "",
  };
}

export function emptyChannelConfig(): ChannelConfig {
  return {
    chatId: "",
    smtpHost: "",
    smtpPort: 587,
    smtpTls: "starttls",
    mailFrom: "",
    mailTo: "",
    username: "",
  };
}
