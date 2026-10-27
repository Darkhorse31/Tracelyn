import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { HowToFix } from "@/components/setup/how-to-fix";
import { Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { SimpleSelect } from "@/components/ui/select";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { usePrivacy } from "@/contexts/privacy-context";
import { useApps } from "@/hooks/use-apps";
import { useLogStream } from "@/hooks/use-log-stream";
import { LOG_LEVELS, type LogLevel, type LogLine } from "@/lib/log-types";
import { cn } from "@/lib/utils";
import { useRememberedState } from "@/hooks/use-remembered-state";

const SAVED_KEY = "tracelyn-log-filters";

interface SavedFilter {
  name: string;
  levels: LogLevel[];
  text: string;
  useRegex: boolean;
  exclude: string;
  appIds: string[];
  range: RangeId;
}

type RangeId = "live" | "15m" | "1h" | "24h" | "custom";

const LEVEL_CLASS: Record<string, string> = {
  fatal: "text-status-critical",
  error: "text-status-critical",
  warn: "text-status-warning",
  info: "text-foreground",
  debug: "text-muted-foreground",
  trace: "text-muted-foreground",
  unknown: "text-muted-foreground",
};

export function LogExplorer({
  serverId,
  lockedAppId,
  levelLock,
  requireHttp = false,
  title = "Logs",
}: {
  serverId: string | null;
  lockedAppId?: string;
  levelLock?: LogLevel[];
  requireHttp?: boolean;
  title?: string;
}) {
  const { apps } = useApps();
  const scope = `logs:${serverId ?? "none"}:${lockedAppId ?? "all"}:${title}`;
  const [storedLevels, setLevels] = useRememberedState<LogLevel[]>(`${scope}:levels`, []);
  const levels = levelLock ?? storedLevels;
  const [storedApps, setPickedApps] = useRememberedState<string[]>(`${scope}:apps`, []);
  const pickedApps = lockedAppId ? [lockedAppId] : storedApps;
  const [params] = useSearchParams();
  const query = params.get("q") ?? "";
  const [text, setText] = useRememberedState(`${scope}:text`, query);
  useEffect(() => {
    if (params.has("q")) setText(query);
  }, [query, params, setText]);
  const [useRegex, setUseRegex] = useRememberedState(`${scope}:regex`, false);
  const [exclude, setExclude] = useRememberedState(`${scope}:exclude`, "");
  const [range, setRange] = useRememberedState<RangeId>(`${scope}:range`, "live");
  const [customStart, setCustomStart] = useRememberedState(`${scope}:custom-start`, "");
  const [follow, setFollow] = useRememberedState(`${scope}:follow`, true);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [saved, setSaved] = useState<SavedFilter[]>(() => readSaved());
  const [saveName, setSaveName] = useState("");
  const subscribeIds = lockedAppId ? [lockedAppId] : pickedApps;
  const { lines, facets, sourceStatus, paused, togglePause, loading } = useLogStream(
    serverId,
    subscribeIds,
  );

  const regexError = useMemo(() => {
    if (!useRegex || !text) return null;
    try {
      new RegExp(text);
      return null;
    } catch (err) {
      return String(err);
    }
  }, [useRegex, text]);

  const filtered = useMemo(() => {
    const start = rangeStart(range, customStart);
    let pattern: RegExp | null = null;
    if (text && useRegex && !regexError) {
      try {
        pattern = new RegExp(text, "i");
      } catch {
        pattern = null;
      }
    }
    const needle = text.toLowerCase();
    const excludeNeedle = exclude.toLowerCase();
    return lines.filter((line) => {
      if (requireHttp && line.httpStatus == null) return false;
      if (lockedAppId && line.appId !== lockedAppId) return false;
      if (!lockedAppId && pickedApps.length && line.appId && !pickedApps.includes(line.appId)) {
        return false;
      }
      if (levels.length && !levels.includes(line.level)) return false;
      if (start && line.tsMs < start) return false;
      if (excludeNeedle && line.raw.toLowerCase().includes(excludeNeedle)) return false;
      if (!text) return true;
      if (pattern) return pattern.test(line.raw);
      return line.raw.toLowerCase().includes(needle);
    });
  }, [lines, lockedAppId, pickedApps, levels, range, customStart, text, useRegex, regexError, exclude, requireHttp]);

  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => (expanded[filtered[index]?.id] ? 160 : 28),
    overscan: 16,
  });

  useEffect(() => {
    if (!follow || paused || filtered.length === 0) return;
    virtualizer.scrollToIndex(filtered.length - 1, { align: "end" });
  }, [follow, paused, filtered.length, virtualizer]);

  const problems = Object.entries(sourceStatus).filter(([, st]) => !st.ok);

  const toggleLevel = (level: LogLevel) => {
    if (levelLock) return;
    setLevels((prev) =>
      prev.includes(level) ? prev.filter((l) => l !== level) : [...prev, level],
    );
  };

  const jumpTo = (value: string) => {
    const ms = Date.parse(value);
    if (Number.isNaN(ms)) return;
    const idx = filtered.findIndex((line) => line.tsMs >= ms);
    if (idx >= 0) virtualizer.scrollToIndex(idx, { align: "start" });
  };

  const persist = () => {
    const name = saveName.trim();
    if (!name) return;
    const next = [
      ...saved.filter((f) => f.name !== name),
      { name, levels, text, useRegex, exclude, appIds: pickedApps, range },
    ];
    setSaved(next);
    localStorage.setItem(SAVED_KEY, JSON.stringify(next));
    setSaveName("");
    toast.success(`Saved filter “${name}”`);
  };

  const applySaved = (filter: SavedFilter) => {
    if (!levelLock) setLevels(filter.levels);
    if (!lockedAppId) setPickedApps(filter.appIds);
    setText(filter.text);
    setUseRegex(filter.useRegex);
    setExclude(filter.exclude);
    setRange(filter.range);
  };

  return (
    <section className="flex flex-col gap-3" aria-label={title}>
      <div className="flex flex-wrap items-end gap-3">
        <div className="grid gap-1">
          <Label htmlFor="log-search">Search</Label>
          <Input
            id="log-search"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={useRegex ? "Regular expression" : "Text"}
            className="w-64"
          />
        </div>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={useRegex}
            onChange={(e) => setUseRegex(e.target.checked)}
          />
          Regex
        </label>
        <div className="grid gap-1">
          <Label htmlFor="log-exclude">Exclude</Label>
          <Input
            id="log-exclude"
            value={exclude}
            onChange={(e) => setExclude(e.target.value)}
            className="w-48"
          />
        </div>
        <div className="grid gap-1">
          <Label htmlFor="log-range">Time</Label>
          <SimpleSelect
            id="log-range"
            value={range}
            onValueChange={(next) => setRange(next as RangeId)}
            options={[
              { value: "live", label: "Live" },
              { value: "15m", label: "Last 15 minutes" },
              { value: "1h", label: "Last hour" },
              { value: "24h", label: "Last 24 hours" },
              { value: "custom", label: "Custom" },
            ]}
          />
        </div>
        {range === "custom" && (
          <div className="grid gap-1">
            <Label htmlFor="log-from">From</Label>
            <Input
              id="log-from"
              type="datetime-local"
              value={customStart}
              onChange={(e) => setCustomStart(e.target.value)}
            />
          </div>
        )}
        <div className="grid gap-1">
          <Label htmlFor="log-jump">Jump to</Label>
          <Input
            id="log-jump"
            type="datetime-local"
            onChange={(e) => jumpTo(e.target.value)}
          />
        </div>
        <Button type="button" variant="outline" size="sm" aria-pressed={paused} onClick={() => void togglePause()}>
          {paused ? "Resume" : "Pause"}
        </Button>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} />
          Follow
        </label>
      </div>

      {regexError && (
        <Notice tone="warning" title="Invalid regular expression">
          {regexError}
        </Notice>
      )}

      <div className="flex flex-wrap gap-2" role="group" aria-label="Level filters">
        {LOG_LEVELS.map((level) => (
          <button
            key={level}
            type="button"
            disabled={Boolean(levelLock)}
            aria-pressed={levels.includes(level)}
            onClick={() => toggleLevel(level)}
            className={cn(
              "rounded-md border border-border px-2 py-1 text-xs",
              levels.includes(level) && "bg-muted font-medium",
            )}
          >
            {level} {facets.level[level] ?? 0}
          </button>
        ))}
      </div>

      {!lockedAppId && (
        <div className="flex flex-wrap gap-3" role="group" aria-label="Applications">
          {apps.map((app) => (
            <label key={app.id} className="flex items-center gap-1.5 text-xs">
              <input
                type="checkbox"
                checked={pickedApps.includes(app.id)}
                onChange={() =>
                  setPickedApps((prev) =>
                    prev.includes(app.id) ? prev.filter((id) => id !== app.id) : [...prev, app.id],
                  )
                }
              />
              {app.name}
              <span className="text-muted-foreground">{facets.appId[app.id] ?? 0}</span>
            </label>
          ))}
        </div>
      )}

      <div className="flex flex-wrap items-end gap-2">
        <div className="grid gap-1">
          <Label htmlFor="save-filter">Save filter</Label>
          <Input
            id="save-filter"
            value={saveName}
            onChange={(e) => setSaveName(e.target.value)}
            className="w-40"
          />
        </div>
        <Button type="button" variant="outline" size="sm" onClick={persist} disabled={!saveName.trim()}>
          Save
        </Button>
        {saved.map((filter) => (
          <Button
            key={filter.name}
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => applySaved(filter)}
          >
            {filter.name}
          </Button>
        ))}
      </div>

      {problems.map(([id, status]) => (
        <Notice
          key={id}
          tone="critical"
          title={`${id}: ${status.reason ?? "unavailable"}`}
          action={<HowToFix text={`${id}: ${status.reason ?? ""} ${status.fixHint ?? ""}`} />}
        >
          {status.fixHint}
        </Notice>
      ))}

      {loading ? (
        <Skeleton className="h-[420px] w-full" />
      ) : (
        <div
          ref={parentRef}
          role="log"
          aria-label={title}
          aria-live={paused ? "off" : "polite"}
          tabIndex={0}
          className="h-[min(560px,60vh)] overflow-auto rounded-lg border border-border bg-background/40 font-mono text-xs focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        >
          {filtered.length === 0 ? (
            <p className="p-4 font-sans text-sm text-muted-foreground">
              {problems.length
                ? "No lines yet. The source status above explains why the stream is empty."
                : "No log lines match the current filters."}
            </p>
          ) : (
            <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
              {virtualizer.getVirtualItems().map((item) => {
                const line = filtered[item.index];
                return (
                  <LogRow
                    key={line.id}
                    line={line}
                    query={text}
                    useRegex={useRegex && !regexError}
                    expanded={Boolean(expanded[line.id])}
                    apps={apps}
                    onToggle={() =>
                      setExpanded((prev) => ({ ...prev, [line.id]: !prev[line.id] }))
                    }
                    measureRef={virtualizer.measureElement}
                    index={item.index}
                    start={item.start}
                  />
                );
              })}
            </div>
          )}
        </div>
      )}
    </section>
  );
}

function LogRow({
  line,
  query,
  useRegex,
  expanded,
  apps,
  onToggle,
  measureRef,
  index,
  start,
}: {
  line: LogLine;
  query: string;
  useRegex: boolean;
  expanded: boolean;
  apps: { id: string; name: string }[];
  onToggle: () => void;
  measureRef: (el: Element | null) => void;
  index: number;
  start: number;
}) {
  const { enabled, mask } = usePrivacy();
  const appName = apps.find((a) => a.id === line.appId)?.name;
  const json = line.raw.trim().startsWith("{");
  let pretty = line.raw;
  if (expanded && json) {
    try {
      pretty = JSON.stringify(JSON.parse(line.raw), null, 2);
    } catch {
      pretty = line.raw;
    }
  }
  return (
    <div
      ref={measureRef}
      data-index={index}
      className="absolute left-0 w-full px-3 py-1 hover:bg-muted/40"
      style={{ transform: `translateY(${start}px)` }}
    >
      <div className="flex items-start gap-2">
        <time className="w-20 shrink-0 text-muted-foreground" dateTime={new Date(line.tsMs).toISOString()}>
          {new Date(line.tsMs).toLocaleTimeString()}
        </time>
        <span className={cn("w-14 shrink-0 uppercase", LEVEL_CLASS[line.level] ?? "")}>{line.level}</span>
        {appName && <span className="w-28 shrink-0 truncate text-muted-foreground">{appName}</span>}
        <span className="w-28 shrink-0 truncate text-muted-foreground">{line.sourceId}</span>
        {line.httpStatus ? (
          <span className="w-10 shrink-0">{line.httpStatus}</span>
        ) : null}
        <span className={cn("min-w-0 flex-1 whitespace-pre-wrap break-all", enabled && "privacy-blur")} data-privacy-mask={enabled ? "" : undefined}>
          {expanded ? mask(pretty) : highlight(mask(line.message), query, useRegex)}
        </span>
        <button type="button" className="shrink-0 underline-offset-2 hover:underline" onClick={() => void navigator.clipboard.writeText(mask(line.raw)).then(() => toast.success("Copied line"))}>
          Copy
        </button>
        {json && (
          <button type="button" className="shrink-0 underline-offset-2 hover:underline" onClick={onToggle}>
            {expanded ? "Collapse" : "JSON"}
          </button>
        )}
      </div>
    </div>
  );
}

function highlight(text: string, query: string, useRegex: boolean) {
  if (!query) return text;
  try {
    const re = useRegex ? new RegExp(query, "ig") : new RegExp(escapeReg(query), "ig");
    const parts: ReactNode[] = [];
    let last = 0;
    let match: RegExpExecArray | null;
    let i = 0;
    while ((match = re.exec(text))) {
      parts.push(text.slice(last, match.index));
      parts.push(
        <mark key={i} className="bg-status-warning/40 text-foreground">
          {match[0]}
        </mark>,
      );
      i += 1;
      last = match.index + match[0].length;
      if (match[0].length === 0) re.lastIndex += 1;
    }
    parts.push(text.slice(last));
    return parts;
  } catch {
    return text;
  }
}

function escapeReg(value: string) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function rangeStart(range: RangeId, customStart: string): number | null {
  const now = Date.now();
  if (range === "15m") return now - 15 * 60_000;
  if (range === "1h") return now - 60 * 60_000;
  if (range === "24h") return now - 24 * 60 * 60_000;
  if (range === "custom" && customStart) {
    const ms = Date.parse(customStart);
    return Number.isNaN(ms) ? null : ms;
  }
  return null;
}

function readSaved(): SavedFilter[] {
  try {
    const raw = localStorage.getItem(SAVED_KEY);
    return raw ? (JSON.parse(raw) as SavedFilter[]) : [];
  } catch {
    return [];
  }
}
