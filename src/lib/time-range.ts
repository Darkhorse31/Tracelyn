export type TimeRangePreset = "live" | "15m" | "1h" | "24h" | "7d" | "custom";

export interface TimeRangeSelection {
  preset: TimeRangePreset;
  /** Unix ms; set when preset is custom or derived from preset. */
  startMs: number;
  endMs: number;
}

const PRESET_MS: Record<Exclude<TimeRangePreset, "live" | "custom">, number> = {
  "15m": 15 * 60 * 1000,
  "1h": 60 * 60 * 1000,
  "24h": 24 * 60 * 60 * 1000,
  "7d": 7 * 24 * 60 * 60 * 1000,
};

export function resolveTimeRange(
  preset: TimeRangePreset,
  nowMs = Date.now(),
  custom?: { startMs: number; endMs: number },
): TimeRangeSelection {
  if (preset === "live") {
    const window = PRESET_MS["15m"];
    return { preset, startMs: nowMs - window, endMs: nowMs };
  }
  if (preset === "custom" && custom) {
    return { preset, startMs: custom.startMs, endMs: custom.endMs };
  }
  if (preset in PRESET_MS) {
    const span = PRESET_MS[preset as keyof typeof PRESET_MS];
    return { preset, startMs: nowMs - span, endMs: nowMs };
  }
  const span = PRESET_MS["1h"];
  return { preset: "1h", startMs: nowMs - span, endMs: nowMs };
}

export const TIME_RANGE_LABELS: Record<TimeRangePreset, string> = {
  live: "Live",
  "15m": "Last 15 minutes",
  "1h": "Last 1 hour",
  "24h": "Last 24 hours",
  "7d": "Last 7 days",
  custom: "Custom range",
};
