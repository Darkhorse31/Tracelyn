export type RuleKind =
  | "server_unreachable"
  | "cpu_high"
  | "memory_high"
  | "disk_high"
  | "http_5xx_rate"
  | "endpoint_p95"
  | "redis_memory_near_max";

export interface RuleParams {
  threshold: number;
  clearThreshold?: number;
  durationSecs?: number;
  metricKey?: string;
  endpointLabel?: string;
}

export interface AlertRule {
  id: string;
  serverId?: string | null;
  kind: RuleKind;
  name: string;
  enabled: boolean;
  params: RuleParams;
  createdAtMs: number;
  updatedAtMs: number;
}

export interface AlertEventRecord {
  id: string;
  ruleId: string;
  ruleName: string;
  serverId: string;
  metricKey?: string | null;
  severity: string;
  message: string;
  value: number;
  threshold: number;
  firedAtMs: number;
  resolvedAtMs?: number | null;
}

/** Payload of the `alert-transition` event emitted when a rule fires or clears. */
export interface AlertTransition {
  ruleId: string;
  ruleName: string;
  serverId: string;
  fired: boolean;
  message: string;
  value: number;
  threshold: number;
}

export interface RetentionConfig {
  rawRetentionDays: number;
  rollupRetentionDays: number;
  rollupIntervalSecs: number;
}
