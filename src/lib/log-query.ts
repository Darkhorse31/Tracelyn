import type { LogLine } from "./log-types.ts";

export const LOG_FIELDS = [
  "level",
  "app",
  "source",
  "status",
  "message",
  "trace_id",
  "span_id",
] as const;

export type LogField = (typeof LOG_FIELDS)[number];
export type Cmp = "=" | ">" | ">=" | "<" | "<=";

export type QueryTerm =
  | { kind: "phrase"; text: string; neg: boolean }
  | { kind: "field"; field: string; value: string; neg: boolean }
  | { kind: "cmp"; field: string; op: Cmp; value: number; neg: boolean };

export interface ParsedQuery {
  terms: QueryTerm[];
  countBy: string | null;
}

export interface TraceHit {
  traceId: string | null;
  spanId: string | null;
}

const UUID =
  /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi;
const IPV4 =
  /\b(?:(?:25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(?:25[0-5]|2[0-4]\d|1?\d?\d)\b/g;

export function usesQueryLanguage(text: string): boolean {
  if (!text.trim()) return false;
  if (text.includes('"') || text.includes("|")) return true;
  return /(?:^|\s)-?[A-Za-z_][\w]*\s*(?:>=|<=|:|>|<)/.test(text);
}

export function parseLogQuery(input: string): ParsedQuery {
  const pipe = input.split("|");
  const head = pipe[0] ?? "";
  let countBy: string | null = null;
  if (pipe.length > 1) {
    const tail = pipe.slice(1).join("|").trim();
    const match = /^count\s+by\s+([A-Za-z_][\w]*)$/i.exec(tail);
    if (match) countBy = match[1].toLowerCase();
  }
  const terms: QueryTerm[] = [];
  for (const token of tokenize(head)) {
    const term = parseToken(token);
    if (term) terms.push(term);
  }
  return { terms, countBy };
}

export function lineMatches(query: ParsedQuery, line: LogLine, appName?: string): boolean {
  return query.terms.every((term) => termMatches(term, line, appName));
}

export function countByField(
  lines: LogLine[],
  field: string,
  appName: (appId: string | null | undefined) => string | undefined,
): { value: string; count: number }[] {
  const map = new Map<string, number>();
  for (const line of lines) {
    const value = fieldText(field, line, appName(line.appId));
    const key = value.trim() ? value : "(empty)";
    map.set(key, (map.get(key) ?? 0) + 1);
  }
  return [...map.entries()]
    .map(([value, count]) => ({ value, count }))
    .sort((a, b) => b.count - a.count || a.value.localeCompare(b.value));
}

/** Text that can be sent to grep or journalctl. Null when the query is local-only. */
export function grepNeedle(query: ParsedQuery): string | null {
  const bits: string[] = [];
  for (const term of query.terms) {
    if (term.neg || term.kind === "cmp") continue;
    if (term.kind === "phrase") bits.push(term.text);
    else if (term.field === "message" || term.field === "trace_id" || term.field === "span_id") {
      bits.push(term.value);
    }
  }
  const needle = bits.join(" ").trim();
  if (!needle) return null;
  if (query.terms.some((term) => term.kind === "cmp")) return needle;
  return needle;
}

export function fieldSuggestions(text: string): string[] {
  const token = text.split(/\s+/).pop() ?? "";
  if (!token || token.includes('"')) return [];
  const bare = token.replace(/^-/, "").split(/[:=<>]/)[0] ?? "";
  if (!bare) return [];
  return LOG_FIELDS.filter((field) => field.startsWith(bare.toLowerCase()));
}

export function patternOf(line: string): string {
  return line.replace(UUID, "<uuid>").replace(IPV4, "<ip>").replace(/\b\d+\b/g, "<n>");
}

export function groupPatterns(lines: string[]): { pattern: string; count: number }[] {
  const map = new Map<string, number>();
  for (const line of lines) {
    const pattern = patternOf(line).trim();
    if (!pattern) continue;
    map.set(pattern, (map.get(pattern) ?? 0) + 1);
  }
  return [...map.entries()]
    .map(([pattern, count]) => ({ pattern, count }))
    .sort((a, b) => b.count - a.count || a.pattern.localeCompare(b.pattern));
}

export function extractTrace(line: string): TraceHit {
  const parent = /00-([0-9a-f]{32})-([0-9a-f]{16})-[0-9a-f]{2}/i.exec(line);
  if (parent) return { traceId: parent[1], spanId: parent[2] };
  return {
    traceId: named(line, ["trace_id", "traceId", "traceID"]),
    spanId: named(line, ["span_id", "spanId", "spanID"]),
  };
}

function named(line: string, names: string[]): string | null {
  for (const name of names) {
    const json = new RegExp(`"${name}"\\s*:\\s*"([^"]*)"`, "i").exec(line);
    if (json?.[1]) return json[1];
    const eq = new RegExp(`(?:^|\\s)${name}=([A-Za-z0-9_-]+)`, "i").exec(line);
    if (eq?.[1]) return eq[1];
  }
  return null;
}

function tokenize(input: string): string[] {
  const out: string[] = [];
  let i = 0;
  while (i < input.length) {
    while (input[i] === " ") i += 1;
    if (i >= input.length) break;
    const quoted = input[i] === '"' || (input[i] === "-" && input[i + 1] === '"');
    if (quoted) {
      const start = input[i] === "-" ? i + 1 : i;
      const end = input.indexOf('"', start + 1);
      if (end < 0) {
        out.push(input.slice(i));
        break;
      }
      out.push(input.slice(i, end + 1));
      i = end + 1;
      continue;
    }
    let j = i;
    while (j < input.length && input[j] !== " ") j += 1;
    out.push(input.slice(i, j));
    i = j;
  }
  return out;
}

function parseToken(token: string): QueryTerm | null {
  let neg = false;
  let body = token;
  if (body.startsWith("-") && body.length > 1 && !body.startsWith("--")) {
    neg = true;
    body = body.slice(1);
  }
  if (body.startsWith('"') && body.endsWith('"') && body.length >= 2) {
    const text = body.slice(1, -1);
    return text ? { kind: "phrase", text, neg } : null;
  }
  const cmp = /^([A-Za-z_][\w]*)\s*(>=|<=|>|<|=)\s*(-?\d+(?:\.\d+)?)$/.exec(body);
  if (cmp) {
    return {
      kind: "cmp",
      field: cmp[1].toLowerCase(),
      op: cmp[2] as Cmp,
      value: Number(cmp[3]),
      neg,
    };
  }
  const field = /^([A-Za-z_][\w]*):(.*)$/.exec(body);
  if (field && field[2]) {
    return { kind: "field", field: field[1].toLowerCase(), value: field[2], neg };
  }
  if (!body) return null;
  return { kind: "phrase", text: body, neg };
}

function termMatches(term: QueryTerm, line: LogLine, appName?: string): boolean {
  const ok = positive(term, line, appName);
  return term.neg ? !ok : ok;
}

function positive(term: QueryTerm, line: LogLine, appName?: string): boolean {
  if (term.kind === "phrase") return line.raw.toLowerCase().includes(term.text.toLowerCase());
  if (term.kind === "cmp") {
    const value = fieldNumber(term.field, line);
    if (value == null) return false;
    if (term.op === ">") return value > term.value;
    if (term.op === ">=") return value >= term.value;
    if (term.op === "<") return value < term.value;
    if (term.op === "<=") return value <= term.value;
    return value === term.value;
  }
  const text = fieldText(term.field, line, appName).toLowerCase();
  const want = term.value.toLowerCase();
  if (term.field === "level") return text === want;
  return text.includes(want);
}

function fieldNumber(field: string, line: LogLine): number | null {
  if (field === "status" || field === "http_status") return line.httpStatus ?? null;
  return null;
}

function fieldText(field: string, line: LogLine, appName?: string): string {
  if (field === "level") return line.level;
  if (field === "app" || field === "app_id") return appName || line.appId || "";
  if (field === "source" || field === "source_id") return line.sourceId;
  if (field === "status" || field === "http_status") return line.httpStatus == null ? "" : String(line.httpStatus);
  if (field === "message") return line.message || line.raw;
  if (field === "trace_id" || field === "traceid") return extractTrace(line.raw).traceId ?? "";
  if (field === "span_id" || field === "spanid") return extractTrace(line.raw).spanId ?? "";
  return "";
}
