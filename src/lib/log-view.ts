import type { LogLevel, LogLine } from "./log-types.ts";

export type RangeId = "live" | "5m" | "15m" | "1h" | "24h" | "7d" | "custom";

export const RANGE_OPTIONS: { id: RangeId; label: string }[] = [
  { id: "live", label: "Live" },
  { id: "5m", label: "5m" },
  { id: "15m", label: "15m" },
  { id: "1h", label: "1h" },
  { id: "24h", label: "24h" },
  { id: "7d", label: "7d" },
  { id: "custom", label: "Custom" },
];

const RANGE_MS: Partial<Record<RangeId, number>> = {
  "5m": 5 * 60_000,
  "15m": 15 * 60_000,
  "1h": 60 * 60_000,
  "24h": 24 * 60 * 60_000,
  "7d": 7 * 24 * 60 * 60_000,
};

export type SourceGroupId = "apps" | "web" | "data" | "system";

export const SOURCE_GROUPS: { id: SourceGroupId; label: string }[] = [
  { id: "apps", label: "Apps (PM2)" },
  { id: "web", label: "Web server sites" },
  { id: "data", label: "Databases and caches" },
  { id: "system", label: "System services" },
];

const DATA_STORE =
  /mysql|mariadb|postgres|postgresql|mongo|redis|memcache|rabbitmq|valkey|clickhouse|elasticsearch|opensearch/i;

export interface SavedFilter {
  name: string;
  levels: LogLevel[];
  text: string;
  useRegex: boolean;
  exclude: string;
  appIds: string[];
  range: RangeId;
}

export const SAVED_FILTERS_KEY = "tracelyn-log-filters";

export function rangeStart(range: RangeId, customStart: string, now = Date.now()): number | null {
  const windowMs = RANGE_MS[range];
  if (windowMs) return now - windowMs;
  if (range === "custom" && customStart) {
    const ms = Date.parse(customStart);
    return Number.isNaN(ms) ? null : ms;
  }
  return null;
}

export function queryRegexError(text: string, useRegex: boolean): string | null {
  if (!useRegex || !text) return null;
  try {
    new RegExp(text);
    return null;
  } catch (err) {
    return String(err);
  }
}

/** Minus-words are excludes. A query with none of them stays an exact phrase. */
export function splitQuery(text: string, useRegex: boolean): { needle: string; excludes: string[] } {
  if (useRegex) return { needle: text, excludes: [] };
  const excludes: string[] = [];
  const parts: string[] = [];
  let sawExclude = false;
  for (const part of text.split(/\s+/)) {
    if (!part) continue;
    if (part.startsWith("-") && part.length > 1) {
      excludes.push(part.slice(1));
      sawExclude = true;
    } else {
      parts.push(part);
    }
  }
  if (!sawExclude) return { needle: text, excludes };
  return { needle: parts.join(" "), excludes };
}

export function sourceGroup(kind: string, name: string): SourceGroupId {
  if (kind === "nginx" || kind === "apache" || /^(apache|nginx):/i.test(name)) return "web";
  if (kind === "pm2") return "apps";
  if (DATA_STORE.test(name)) return "data";
  if (kind === "docker" || kind === "manual") return "apps";
  return "system";
}

export function visibleSources<T extends { id: string; name: string }>(
  apps: T[],
  counts: Record<string, number>,
  query: string,
  showEmpty: boolean,
): T[] {
  const needle = query.trim().toLowerCase();
  return apps.filter((app) => {
    if (!showEmpty && (counts[app.id] ?? 0) === 0) return false;
    if (needle && !app.name.toLowerCase().includes(needle)) return false;
    return true;
  });
}

export function groupSources<T extends { id: string; kind: string; name: string }>(
  apps: T[],
): { id: SourceGroupId; label: string; items: T[] }[] {
  const buckets: Record<SourceGroupId, T[]> = { apps: [], web: [], data: [], system: [] };
  for (const app of apps) buckets[sourceGroup(app.kind, app.name)].push(app);
  return SOURCE_GROUPS.map((group) => ({
    ...group,
    items: buckets[group.id].slice().sort((a, b) => a.name.localeCompare(b.name)),
  })).filter((group) => group.items.length > 0);
}

export function toggleSourceGroup(selected: string[], ids: string[]): string[] {
  if (ids.length === 0) return selected;
  if (ids.every((id) => selected.includes(id))) return selected.filter((id) => !ids.includes(id));
  const next = new Set(selected);
  for (const id of ids) next.add(id);
  return [...next];
}

export function filterLogLines(input: {
  lines: LogLine[];
  requireHttp?: boolean;
  lockedAppId?: string;
  pickedApps: string[];
  levels: LogLevel[];
  range: RangeId;
  customStart: string;
  text: string;
  useRegex: boolean;
  exclude: string;
  now?: number;
}): LogLine[] {
  const start = rangeStart(input.range, input.customStart, input.now);
  const error = queryRegexError(input.text, input.useRegex);
  const parsed = splitQuery(input.text, input.useRegex);
  let pattern: RegExp | null = null;
  if (input.useRegex && input.text && !error) {
    try {
      pattern = new RegExp(input.text, "i");
    } catch {
      pattern = null;
    }
  }
  const needle = parsed.needle.toLowerCase();
  const banned = [input.exclude, ...parsed.excludes]
    .map((term) => term.trim().toLowerCase())
    .filter(Boolean);
  return input.lines.filter((line) => {
    if (input.requireHttp && line.httpStatus == null) return false;
    if (input.lockedAppId && line.appId !== input.lockedAppId) return false;
    if (
      !input.lockedAppId &&
      input.pickedApps.length > 0 &&
      line.appId &&
      !input.pickedApps.includes(line.appId)
    ) {
      return false;
    }
    if (input.levels.length > 0 && !input.levels.includes(line.level)) return false;
    if (start != null && line.tsMs < start) return false;
    const raw = line.raw.toLowerCase();
    if (banned.some((term) => raw.includes(term))) return false;
    if (!parsed.needle) return true;
    if (pattern) return pattern.test(line.raw);
    return raw.includes(needle);
  });
}

export function readSavedFilters(raw: string | null): SavedFilter[] {
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isSavedFilter);
  } catch {
    return [];
  }
}

export function upsertSavedFilter(saved: SavedFilter[], filter: SavedFilter): SavedFilter[] {
  const name = filter.name.trim();
  if (!name) return saved;
  return [...saved.filter((item) => item.name !== name), { ...filter, name }];
}

export function renameSavedFilter(
  saved: SavedFilter[],
  from: string,
  to: string,
): SavedFilter[] | null {
  const name = to.trim();
  if (!name || name === from) return saved;
  if (saved.some((item) => item.name === name)) return null;
  return saved.map((item) => (item.name === from ? { ...item, name } : item));
}

export function deleteSavedFilter(saved: SavedFilter[], name: string): SavedFilter[] {
  return saved.filter((item) => item.name !== name);
}

function isSavedFilter(value: unknown): value is SavedFilter {
  if (!value || typeof value !== "object") return false;
  const row = value as Partial<SavedFilter>;
  return (
    typeof row.name === "string" &&
    typeof row.text === "string" &&
    typeof row.useRegex === "boolean" &&
    typeof row.exclude === "string" &&
    Array.isArray(row.appIds) &&
    Array.isArray(row.levels) &&
    typeof row.range === "string"
  );
}
