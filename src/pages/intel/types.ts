export type IntelKind =
  | "metric"
  | "log_pattern"
  | "error_rate"
  | "p95"
  | "ssl_expiry"
  | "uptime";

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
};

export const CHANNEL_LABELS: Record<ChannelKind, string> = {
  windows: "Windows notification",
  slack: "Slack",
  discord: "Discord",
  teams: "Microsoft Teams",
  telegram: "Telegram",
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

export function emptyParams(kind: IntelKind): RuleParams {
  const low = kind === "ssl_expiry" || kind === "uptime";
  return {
    threshold: kind === "ssl_expiry" ? 14 : kind === "uptime" ? 0 : kind === "p95" ? 500 : 90,
    clearThreshold: low ? (kind === "ssl_expiry" ? 21 : 0) : undefined,
    durationSecs: kind === "uptime" || kind === "ssl_expiry" ? 0 : 60,
    clearDurationSecs: 30,
    metricKey: kind === "error_rate" ? "http.5xx_rate_percent" : kind === "p95" ? "endpoint.p95_ms" : "cpu.percent",
    pattern: "",
    windowSecs: 60,
    host: "",
    port: kind === "ssl_expiry" ? 443 : kind === "uptime" ? 80 : 0,
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
