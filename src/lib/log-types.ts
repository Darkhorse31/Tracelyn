export type LogLevel =
  | "trace"
  | "debug"
  | "info"
  | "warn"
  | "error"
  | "fatal"
  | "unknown";

export interface LogLine {
  id: string;
  tsMs: number;
  appId?: string | null;
  sourceId: string;
  level: LogLevel;
  message: string;
  raw: string;
  httpStatus?: number | null;
  multilineGroupId?: string | null;
}

export interface SourceStatus {
  ok: boolean;
  reason?: string | null;
  fixHint?: string | null;
}

export interface LogStreamSnapshot {
  lines: LogLine[];
  incremental: boolean;
  paused: boolean;
  sourceStatus: Record<string, SourceStatus>;
  facets: {
    level: Record<string, number>;
    appId: Record<string, number>;
  };
}

export const LOG_LEVELS: LogLevel[] = [
  "fatal",
  "error",
  "warn",
  "info",
  "debug",
  "trace",
  "unknown",
];
