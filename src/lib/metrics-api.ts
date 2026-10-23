import { invoke } from "@tauri-apps/api/core";

export interface StoredMetricPoint {
  tsMs: number;
  value: number;
  min?: number;
  max?: number;
}

export interface QueryMetricResult {
  serverId: string;
  metricKey: string;
  resolution: string;
  points: StoredMetricPoint[];
}

export async function queryMetricSeries(input: {
  serverId: string;
  metricKey: string;
  rangeStartMs: number;
  rangeEndMs: number;
  preferLive: boolean;
}): Promise<QueryMetricResult> {
  return invoke<QueryMetricResult>("query_metric_series", {
    input: {
      serverId: input.serverId,
      metricKey: input.metricKey,
      rangeStartMs: input.rangeStartMs,
      rangeEndMs: input.rangeEndMs,
      preferLive: input.preferLive,
    },
  });
}
