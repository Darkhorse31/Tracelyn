export interface TraceSummary {
  traceId: string;
  service: string;
  operation: string;
  startMs: number;
  durationUs: number;
  status: string;
  spanCount: number;
  appId?: string | null;
}

export interface SpanDetail {
  spanId: string;
  parentSpanId?: string | null;
  service: string;
  operation: string;
  kind: string;
  status: string;
  startMs: number;
  durationUs: number;
  appId?: string | null;
  peer?: string | null;
  attributes: Record<string, unknown>;
  events: SpanEvent[];
  depth: number;
  critical: boolean;
}

export interface SpanEvent {
  name: string;
  timeMs: number;
  attributes: Record<string, unknown>;
}

export interface RelatedLog {
  tsMs: number;
  severity: string;
  body: string;
  spanId?: string | null;
  service: string;
}

export interface TraceDetail {
  traceId: string;
  spans: SpanDetail[];
  logs: RelatedLog[];
  errorFingerprints: string[];
  criticalPath: string[];
  truncated: boolean;
}

export interface MapNode {
  id: string;
  label: string;
  spanCount: number;
  errorCount: number;
}

export interface MapEdge {
  source: string;
  target: string;
  requests: number;
  errors: number;
  ratePerSec: number;
  errorRate: number;
  p95Ms: number;
}

export interface ServiceMap {
  nodes: MapNode[];
  edges: MapEdge[];
  windowMs: number;
  truncated: boolean;
}

export interface ErrorIssue {
  fingerprint: string;
  appId?: string | null;
  title: string;
  language: string;
  firstSeenMs: number;
  lastSeenMs: number;
  count: number;
  state: string;
  sampleMessage: string;
  sampleStack: string;
  traceIds: string[];
  sparkline: number[];
}

export interface SlowQueryRow {
  engine: string;
  fingerprint: string;
  normalized: string;
  appId?: string | null;
  count: number;
  totalMs: number;
  avgMs: number;
  p95Ms: number;
  rowsExaminedAvg: number;
  lastSeenMs: number;
  sample: string;
}

export interface OtelSuggestion {
  path: string;
  appId: string;
  reason: string;
}

export interface OtelPathRow {
  appId: string;
  path: string;
}

export interface SlowEngineStatus {
  engine: string;
  path?: string | null;
  source: string;
  loggingOn: boolean;
  isDir: boolean;
  hint: string;
  snippet: string;
}

export interface OtelStats {
  malformed: number;
  spansIngested: number;
  filesOk: number;
  filesError: number;
  lastError?: string | null;
  reason?: string;
  detail?: string;
  sample?: string | null;
  collector?: string;
  lastWrite?: string | null;
}

export interface InsightSetup {
  paths: OtelPathRow[];
  suggestions: OtelSuggestion[];
  stats: OtelStats;
  engines: SlowEngineStatus[];
}

export interface TraceQuery {
  serverId: string;
  startMs: number;
  endMs: number;
  service?: string;
  operation?: string;
  status?: string;
  minDurationMs?: number;
  traceId?: string;
  appId?: string;
}
