export type StatusBreakdown = Record<string, number>;

export interface WebTrafficMetrics {
  reqPerSec: number;
  bandwidthMbps: number;
  statusBreakdown: StatusBreakdown;
  topUrls: { path: string; count: number }[];
  topIps: { ip: string; count: number }[];
  reqPerSecSeries: { t: number; v: number }[];
  timingNotice?: {
    message: string;
    suggestedLine: string;
  };
}

export interface LiveLogLine {
  id: string;
  timestamp: number;
  status: number;
  method: string;
  path: string;
  remoteAddr: string;
  latencyMs?: number;
  raw: string;
}

export interface WebTrafficFilters {
  statusMin?: number;
  statusMax?: number;
  pathQuery: string;
  ipQuery: string;
  textQuery: string;
}

export interface WebTrafficData {
  metrics: WebTrafficMetrics;
  logs: LiveLogLine[];
  loading: boolean;
}
