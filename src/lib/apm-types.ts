export interface EndpointRow {
  route: string;
  throughput: number;
  avgMs: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
  errorRate: number;
  statusBreakdown: Record<string, number>;
}

export interface LatencyHeatmapCell {
  route: string;
  bucketIndex: number;
  p50Ms: number;
  p95Ms: number;
}

export interface RouteRule {
  pattern: string;
  replacement: string;
}

export interface ApmData {
  endpoints: EndpointRow[];
  heatmap: LatencyHeatmapCell[];
  slowest: EndpointRow[];
  mostFailing: EndpointRow[];
  routeRules: RouteRule[];
  latencySeries: {
    route: string;
    points: { t: number; p50: number; p95: number; p99: number }[];
  }[];
  timingNotice?: {
    message: string;
    suggestedLine: string;
  };
  loading: boolean;
}
