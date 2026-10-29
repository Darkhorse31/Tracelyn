import type { LogLevel, LogLine } from "./log-types.ts";

export interface LogGroup {
  id: string;
  level: LogLevel;
  tsMs: number;
  lines: LogLine[];
}

const FIELD =
  /^(length|severity|code|detail|hint|position|internalPosition|internalQuery|where|schema|table|column|dataType|constraint|file|line|routine)\s*[:=]/i;

function isBlank(raw: string): boolean {
  return raw.trim().length === 0;
}

/** Indented frames and language stack lines. */
export function isStackContinuation(raw: string): boolean {
  if (isBlank(raw)) return false;
  if (raw.startsWith(" ") || raw.startsWith("\t")) return true;
  const text = raw.trim();
  return (
    text.startsWith("at ") ||
    text.startsWith("Caused by") ||
    text.startsWith("...") ||
    text.startsWith('File "') ||
    text.startsWith("goroutine ") ||
    /^#\d+\b/.test(text)
  );
}

/** Postgres error fields and the Node crash lines that follow a dump. */
export function isDumpTrailer(raw: string): boolean {
  const text = raw.trim();
  if (!text) return false;
  if (text === "}" || text === "},") return true;
  if (FIELD.test(text)) return true;
  if (/^Node\.js v\d/.test(text)) return true;
  if (/^Segmentation fault\b/.test(text)) return true;
  return false;
}

function trailerContinues(raw: string, previousRaw: string, previousLevel: string): boolean {
  if (!isDumpTrailer(raw)) return false;
  return (
    previousLevel === "error" ||
    previousLevel === "fatal" ||
    isStackContinuation(previousRaw) ||
    isDumpTrailer(previousRaw) ||
    /error|exception|traceback|segfault|segmentation fault/i.test(previousRaw)
  );
}

function sameMultiline(line: LogLine, group: LogGroup): boolean {
  const id = line.multilineGroupId;
  if (!id) return false;
  return group.lines.some((item) => item.multilineGroupId === id);
}

/** Consecutive continuation lines become one entry. The first line of a dump stays the head. */
export function groupLogLines(lines: LogLine[]): LogGroup[] {
  const groups: LogGroup[] = [];
  for (const line of lines) {
    const prev = groups[groups.length - 1];
    const raw = line.raw || line.message;
    const previous = prev?.lines[prev.lines.length - 1];
    const previousRaw = previous ? previous.raw || previous.message : "";
    const attach =
      prev != null &&
      (sameMultiline(line, prev) ||
        isBlank(raw) ||
        isStackContinuation(raw) ||
        trailerContinues(raw, previousRaw, prev.level));
    if (prev && attach) {
      prev.lines.push(line);
      continue;
    }
    groups.push({ id: line.id, level: line.level, tsMs: line.tsMs, lines: [line] });
  }
  return groups;
}

export function groupText(text: string): string[][] {
  const groups: string[][] = [];
  for (const row of text.split(/\r?\n/)) {
    const prev = groups[groups.length - 1];
    const previousRaw = prev?.[prev.length - 1] ?? "";
    const head = prev?.[0] ?? "";
    const level = /error|exception|fatal|segfault|segmentation fault/i.test(`${head}\n${previousRaw}`) ? "error" : "info";
    const attach =
      prev != null && (isBlank(row) || isStackContinuation(row) || trailerContinues(row, previousRaw, level));
    if (prev && attach) prev.push(row);
    else groups.push([row]);
  }
  return groups.filter((group) => group.some((row) => row.trim().length > 0));
}

export function groupBody(group: LogGroup): string {
  return group.lines.map((line) => line.raw || line.message).join("\n");
}

export function firstLine(text: string): string {
  const line = text.split(/\r?\n/).find((row) => row.trim().length > 0);
  return (line ?? "").trim();
}

export function textLineCount(text: string): number {
  return text.split(/\r?\n/).filter((row) => row.trim().length > 0).length;
}
