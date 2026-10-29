import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronDown, ChevronRight, Copy, Pencil, Search, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from "react";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { HowToFix } from "@/components/setup/how-to-fix";
import { Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from "@/components/ui/input-group";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { usePrivacy } from "@/contexts/privacy-context";
import { useApps } from "@/hooks/use-apps";
import { useLogStream } from "@/hooks/use-log-stream";
import { useRememberedState } from "@/hooks/use-remembered-state";
import type { TracelynApp } from "@/lib/apps-types";
import { LOG_LEVELS, type LogLevel, type LogLine } from "@/lib/log-types";
import {
  RANGE_OPTIONS,
  SAVED_FILTERS_KEY,
  deleteSavedFilter,
  filterLogLines,
  groupSources,
  queryRegexError,
  readSavedFilters,
  renameSavedFilter,
  splitQuery,
  toggleSourceGroup,
  upsertSavedFilter,
  visibleSources,
  type RangeId,
  type SavedFilter,
} from "@/lib/log-view";
import { cn } from "@/lib/utils";

const LEVEL_BAR: Record<LogLevel, string> = {
  fatal: "bg-status-critical",
  error: "bg-status-critical",
  warn: "bg-status-warning",
  info: "bg-status-info",
  debug: "bg-muted-foreground",
  trace: "bg-muted-foreground/70",
  unknown: "bg-border",
};

const LEVEL_TEXT: Record<LogLevel, string> = {
  fatal: "text-status-critical",
  error: "text-status-critical",
  warn: "text-status-warning",
  info: "text-status-info",
  debug: "text-muted-foreground",
  trace: "text-muted-foreground",
  unknown: "text-muted-foreground",
};

const LEVEL_ON: Record<LogLevel, string> = {
  fatal: "border-status-critical bg-status-critical/15",
  error: "border-status-critical bg-status-critical/10",
  warn: "border-status-warning bg-status-warning/15",
  info: "border-status-info bg-status-info/15",
  debug: "border-foreground/25 bg-muted",
  trace: "border-foreground/20 bg-muted",
  unknown: "border-border bg-muted",
};

const MENU_ITEM =
  "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-sm outline-none hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring aria-pressed:bg-muted";

function onMenuKeyDown(event: ReactKeyboardEvent<HTMLDivElement>) {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp" && event.key !== "Home" && event.key !== "End") {
    return;
  }
  const items = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
  if (!items.length) return;
  event.preventDefault();
  const current = items.indexOf(document.activeElement as HTMLButtonElement);
  if (event.key === "Home") {
    items[0]?.focus();
    return;
  }
  if (event.key === "End") {
    items[items.length - 1]?.focus();
    return;
  }
  const delta = event.key === "ArrowDown" ? 1 : -1;
  const next = current < 0 ? 0 : (current + delta + items.length) % items.length;
  items[next]?.focus();
}

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
  const [saved, setSaved] = useState<SavedFilter[]>(loadSaved);
  const searchRef = useRef<HTMLInputElement>(null);
  const subscribeIds = lockedAppId ? [lockedAppId] : pickedApps;
  const { lines, facets, sourceStatus, paused, togglePause, loading } = useLogStream(serverId, subscribeIds);
  const regexError = useMemo(() => queryRegexError(text, useRegex), [text, useRegex]);
  const parsed = useMemo(() => splitQuery(text, useRegex), [text, useRegex]);
  const filtered = useMemo(
    () =>
      filterLogLines({
        lines,
        requireHttp,
        lockedAppId,
        pickedApps,
        levels,
        range,
        customStart,
        text,
        useRegex,
        exclude,
      }),
    [lines, requireHttp, lockedAppId, pickedApps, levels, range, customStart, text, useRegex, exclude],
  );

  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (index) => (expanded[filtered[index]?.id] ? 160 : 32),
    overscan: 16,
  });

  useEffect(() => {
    if (!follow || paused || filtered.length === 0) return;
    virtualizer.scrollToIndex(filtered.length - 1, { align: "end" });
  }, [follow, paused, filtered.length, virtualizer]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.isComposing || event.repeat) return;
      if (document.querySelector("[data-slot='dialog-content'][data-open], [data-slot='popover-content'][data-open]")) {
        return;
      }
      const plain = !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey;
      if (!plain) return;
      const target = event.target instanceof Element ? event.target : null;
      const typing = target?.closest("input, textarea, select, [contenteditable=true], [role=combobox], .xterm");
      if (event.key === "/") {
        if (typing) return;
        event.preventDefault();
        event.stopImmediatePropagation();
        searchRef.current?.focus();
        searchRef.current?.select();
        return;
      }
      if (event.key.toLowerCase() === "l") {
        if (typing) return;
        event.preventDefault();
        void togglePause();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [togglePause]);

  const problems = Object.entries(sourceStatus).filter(([, st]) => !st.ok);
  const rangeLabel = RANGE_OPTIONS.find((option) => option.id === range)?.label ?? "Live";

  const toggleLevel = (level: LogLevel) => {
    if (levelLock) return;
    setLevels((prev) => (prev.includes(level) ? prev.filter((item) => item !== level) : [...prev, level]));
  };

  const jumpTo = (value: string) => {
    const ms = Date.parse(value);
    if (Number.isNaN(ms)) return;
    const idx = filtered.findIndex((line) => line.tsMs >= ms);
    if (idx >= 0) virtualizer.scrollToIndex(idx, { align: "start" });
  };

  const commitSaved = (next: SavedFilter[]) => {
    setSaved(next);
    try {
      localStorage.setItem(SAVED_FILTERS_KEY, JSON.stringify(next));
    } catch {
      /* Saved filters are optional when storage is unavailable. */
    }
  };

  const persist = (name: string) => {
    const next = upsertSavedFilter(saved, {
      name,
      levels,
      text,
      useRegex,
      exclude,
      appIds: pickedApps,
      range,
    });
    if (next === saved) return;
    commitSaved(next);
    toast.success(`Saved filter "${name.trim()}"`);
  };

  const applySaved = (filter: SavedFilter) => {
    if (!levelLock) setLevels(filter.levels);
    if (!lockedAppId) setPickedApps(filter.appIds);
    setText(filter.text);
    setUseRegex(filter.useRegex);
    setExclude(filter.exclude);
    setRange(filter.range);
  };

  const renameSaved = (from: string, to: string) => {
    const next = renameSavedFilter(saved, from, to);
    if (!next) {
      toast.error("That filter name already exists");
      return false;
    }
    if (next !== saved) {
      commitSaved(next);
      toast.success(`Renamed filter to "${to.trim()}"`);
    }
    return true;
  };

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-2" aria-label={title}>
      <p className="sr-only">Press / to focus search. Press L to pause or resume the live tail.</p>
      <div role="toolbar" aria-label="Log filters" className="flex flex-wrap items-center gap-2">
        <div className="min-w-48 flex-1 basis-full sm:max-w-md sm:basis-72">
          <InputGroup>
            <InputGroupAddon>
              <Search />
            </InputGroupAddon>
            <InputGroupInput
              ref={searchRef}
              id="log-search"
              value={text}
              placeholder={useRegex ? "Regular expression" : "Search or -term"}
              aria-label="Search logs"
              aria-keyshortcuts="/"
              aria-invalid={regexError ? true : undefined}
              spellCheck={false}
              autoCapitalize="off"
              autoCorrect="off"
              onChange={(event) => setText(event.target.value)}
              onKeyDown={(event) => {
                if (event.key !== "Escape") return;
                event.preventDefault();
                if (text) setText("");
                else event.currentTarget.blur();
              }}
            />
            <InputGroupAddon align="inline-end" className="gap-1">
              <kbd aria-hidden className="border border-border bg-muted px-1 text-xs text-muted-foreground">
                /
              </kbd>
              <InputGroupButton
                aria-pressed={useRegex}
                aria-label={useRegex ? "Regex on" : "Regex off"}
                variant={useRegex ? "secondary" : "ghost"}
                onClick={() => setUseRegex((value) => !value)}
              >
                .*
              </InputGroupButton>
            </InputGroupAddon>
          </InputGroup>
        </div>
        <Input
          id="log-exclude"
          value={exclude}
          aria-label="Exclude"
          placeholder="Exclude"
          onChange={(event) => setExclude(event.target.value)}
          className="w-36"
        />
        <Popover>
          <PopoverTrigger
            render={
              <Button
                type="button"
                variant={range === "live" ? "outline" : "secondary"}
                aria-label={`Time range, ${rangeLabel}`}
              />
            }
          >
            <span className="truncate">{rangeLabel}</span>
            <ChevronDown data-icon="inline-end" />
          </PopoverTrigger>
          <PopoverContent align="start" className="w-44 gap-0 p-1">
            <div role="group" aria-label="Time range" onKeyDown={onMenuKeyDown} className="flex flex-col gap-0.5">
              {RANGE_OPTIONS.map((option) => (
                <button
                  key={option.id}
                  type="button"
                  className={MENU_ITEM}
                  aria-pressed={range === option.id}
                  onClick={() => setRange(option.id)}
                >
                  {option.label}
                </button>
              ))}
            </div>
          </PopoverContent>
        </Popover>
        {range === "custom" && (
          <Input
            id="log-from"
            type="datetime-local"
            aria-label="Custom start"
            value={customStart}
            onChange={(event) => setCustomStart(event.target.value)}
            className="w-48"
          />
        )}
        <Popover>
          <PopoverTrigger render={<Button type="button" variant="outline" aria-label="Jump to a time" />}>
            Jump
            <ChevronDown data-icon="inline-end" />
          </PopoverTrigger>
          <PopoverContent align="start" className="w-auto gap-2 p-3">
            <Input id="log-jump" type="datetime-local" aria-label="Jump to" onChange={(event) => jumpTo(event.target.value)} />
          </PopoverContent>
        </Popover>
        <Button
          type="button"
          variant={paused ? "secondary" : "outline"}
          aria-pressed={paused}
          aria-keyshortcuts="L"
          onClick={() => void togglePause()}
        >
          <span aria-hidden className={cn("size-2 rounded-full", paused ? "bg-muted-foreground" : "bg-status-ok")} />
          {paused ? "Paused" : "Live"}
        </Button>
        <div className="flex h-8 items-center gap-2 px-1">
          <Switch id="log-follow" checked={follow} onCheckedChange={setFollow} />
          <Label htmlFor="log-follow">Follow</Label>
        </div>
        <SavedFilters
          saved={saved}
          onSave={persist}
          onApply={applySaved}
          onRename={renameSaved}
          onDelete={(name) => {
            commitSaved(deleteSavedFilter(saved, name));
            toast.success(`Deleted filter "${name}"`);
          }}
        />
      </div>

      {regexError && (
        <Notice tone="warning" title="Invalid regular expression">
          {regexError}
        </Notice>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <div role="group" aria-label="Level filters" className="flex flex-wrap gap-2">
          {LOG_LEVELS.map((level) => {
            const count = facets.level[level] ?? 0;
            const on = levels.includes(level);
            return (
              <button
                key={level}
                type="button"
                disabled={Boolean(levelLock)}
                aria-pressed={on}
                aria-label={`${level}, ${count}`}
                onClick={() => toggleLevel(level)}
                className={cn(
                  "inline-flex h-8 items-center gap-1.5 rounded-lg border border-border bg-background px-2 text-xs font-medium text-foreground",
                  "focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none",
                  "disabled:pointer-events-none disabled:opacity-50",
                  on && LEVEL_ON[level],
                  count === 0 && !on && "text-muted-foreground",
                )}
              >
                <span aria-hidden className={cn("size-2 rounded-full", LEVEL_BAR[level])} />
                <span className="capitalize">{level}</span>
                <span className="tabular-nums text-muted-foreground">{count}</span>
              </button>
            );
          })}
        </div>
        {!lockedAppId && (
          <SourceMenu
            apps={apps}
            counts={facets.appId}
            selected={pickedApps}
            onChange={setPickedApps}
          />
        )}
      </div>

      {!lockedAppId && pickedApps.length > 0 && (
        <div className="flex flex-wrap gap-2" aria-label="Selected sources">
          {pickedApps.map((id) => {
            const name = apps.find((app) => app.id === id)?.name ?? id;
            return (
              <button
                key={id}
                type="button"
                className="inline-flex h-8 max-w-56 items-center gap-1 rounded-lg border border-border bg-muted px-2 text-xs text-foreground focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none"
                aria-label={`Remove ${name}`}
                onClick={() => setPickedApps((prev) => prev.filter((item) => item !== id))}
              >
                <span className="truncate">{name}</span>
                <X aria-hidden className="size-3 shrink-0" />
              </button>
            );
          })}
        </div>
      )}

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
        <div className="min-h-40 flex-1" role="status" aria-label="Loading logs">
          <Skeleton className="h-full min-h-40 w-full" />
        </div>
      ) : (
        <div
          ref={parentRef}
          role="log"
          aria-label={title}
          aria-live={paused ? "off" : "polite"}
          tabIndex={0}
          className="min-h-[min(35rem,60vh)] w-full min-w-0 flex-1 overflow-auto rounded-lg border border-border bg-background font-mono text-xs text-foreground focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none"
        >
          {filtered.length === 0 ? (
            <p className="flex h-full min-h-40 items-center justify-center p-8 text-center font-sans text-sm text-muted-foreground">
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
                    query={parsed.needle}
                    useRegex={useRegex && !regexError}
                    expanded={Boolean(expanded[line.id])}
                    apps={apps}
                    onToggle={() => setExpanded((prev) => ({ ...prev, [line.id]: !prev[line.id] }))}
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

function SourceMenu({
  apps,
  counts,
  selected,
  onChange,
}: {
  apps: TracelynApp[];
  counts: Record<string, number>;
  selected: string[];
  onChange: (next: string[] | ((prev: string[]) => string[])) => void;
}) {
  const [query, setQuery] = useState("");
  const [showEmpty, setShowEmpty] = useState(false);
  const groups = groupSources(visibleSources(apps, counts, query, showEmpty));
  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            type="button"
            variant={selected.length ? "secondary" : "outline"}
            aria-label={selected.length ? `Sources, ${selected.length} selected` : "Sources"}
          />
        }
      >
        <span className="truncate">Sources</span>
        {selected.length > 0 && (
          <span className="tabular-nums text-muted-foreground">{selected.length}</span>
        )}
        <ChevronDown data-icon="inline-end" />
      </PopoverTrigger>
      <PopoverContent align="start" className="w-80 gap-2 p-2">
        <Input
          aria-label="Search sources"
          placeholder="Search sources"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <div className="flex h-8 items-center justify-between gap-2 px-1">
          <Label htmlFor="show-empty-sources">Show empty</Label>
          <Switch id="show-empty-sources" checked={showEmpty} onCheckedChange={setShowEmpty} />
        </div>
        <div className="max-h-80 overflow-auto" onKeyDown={onMenuKeyDown}>
          {groups.length === 0 ? (
            <p className="px-2 py-4 text-center text-sm text-muted-foreground">
              {query.trim() ? "No matching sources" : "No sources with log lines"}
            </p>
          ) : (
            groups.map((group) => {
              const ids = group.items.map((app) => app.id);
              const allOn = ids.every((id) => selected.includes(id));
              const total = group.items.reduce((sum, app) => sum + (counts[app.id] ?? 0), 0);
              return (
                <div key={group.id} className="mb-2">
                  <div className="flex h-8 items-center gap-2 px-2">
                    <span className="min-w-0 flex-1 truncate text-xs font-medium text-muted-foreground">
                      {group.label}
                    </span>
                    <span className="text-xs tabular-nums text-muted-foreground">{total}</span>
                    <button
                      type="button"
                      className={MENU_ITEM + " w-auto shrink-0 px-2"}
                      aria-pressed={allOn}
                      aria-label={allOn ? `Clear ${group.label}` : `Select all ${group.label}`}
                      onClick={() => onChange(toggleSourceGroup(selected, ids))}
                    >
                      {allOn ? "Clear" : "All"}
                    </button>
                  </div>
                  {group.items.map((app) => {
                    const count = counts[app.id] ?? 0;
                    const on = selected.includes(app.id);
                    return (
                      <label
                        key={app.id}
                        className="flex h-8 cursor-pointer items-center gap-2 rounded-md px-2 text-sm hover:bg-muted"
                      >
                        <input
                          type="checkbox"
                          className="size-4 accent-primary"
                          checked={on}
                          onChange={() =>
                            onChange(on ? selected.filter((id) => id !== app.id) : [...selected, app.id])
                          }
                        />
                        <span className="min-w-0 flex-1 truncate">{app.name}</span>
                        <span className="text-xs tabular-nums text-muted-foreground">{count}</span>
                      </label>
                    );
                  })}
                </div>
              );
            })
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}

function SavedFilters({
  saved,
  onSave,
  onApply,
  onRename,
  onDelete,
}: {
  saved: SavedFilter[];
  onSave: (name: string) => void;
  onApply: (filter: SavedFilter) => void;
  onRename: (from: string, to: string) => boolean;
  onDelete: (name: string) => void;
}) {
  const [name, setName] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const commitRename = () => {
    if (!renaming) return;
    if (onRename(renaming, draft)) setRenaming(null);
  };
  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button type="button" variant="outline" aria-label={saved.length ? `Saved filters, ${saved.length}` : "Saved filters"} />
        }
      >
        <span className="truncate">Saved</span>
        {saved.length > 0 && <span className="tabular-nums text-muted-foreground">{saved.length}</span>}
        <ChevronDown data-icon="inline-end" />
      </PopoverTrigger>
      <PopoverContent align="end" className="w-72 gap-2 p-2">
        <div className="flex items-center gap-2">
          <Input
            id="save-filter"
            aria-label="Filter name"
            placeholder="Filter name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Enter" || !name.trim()) return;
              event.preventDefault();
              onSave(name);
              setName("");
            }}
          />
          <Button
            type="button"
            variant="outline"
            disabled={!name.trim()}
            onClick={() => {
              onSave(name);
              setName("");
            }}
          >
            Save
          </Button>
        </div>
        {saved.length === 0 ? (
          <p className="px-1 py-2 text-sm text-muted-foreground">No saved filters</p>
        ) : (
          <div className="flex max-h-64 flex-col gap-1 overflow-auto" onKeyDown={onMenuKeyDown}>
            {saved.map((filter) =>
              renaming === filter.name ? (
                <div key={filter.name} className="flex items-center gap-1">
                  <Input
                    aria-label={`New name for ${filter.name}`}
                    value={draft}
                    autoFocus
                    onChange={(event) => setDraft(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        commitRename();
                      } else if (event.key === "Escape") {
                        event.preventDefault();
                        setRenaming(null);
                      }
                    }}
                  />
                  <Button type="button" variant="outline" onClick={commitRename}>
                    Rename
                  </Button>
                </div>
              ) : (
                <div key={filter.name} className="flex items-center gap-1">
                  <button type="button" className={MENU_ITEM + " min-w-0 flex-1"} onClick={() => onApply(filter)}>
                    <span className="truncate">{filter.name}</span>
                  </button>
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    aria-label={`Rename ${filter.name}`}
                    onClick={() => {
                      setRenaming(filter.name);
                      setDraft(filter.name);
                    }}
                  >
                    <Pencil />
                  </Button>
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    aria-label={`Delete ${filter.name}`}
                    onClick={() => onDelete(filter.name)}
                  >
                    <Trash2 />
                  </Button>
                </div>
              ),
            )}
          </div>
        )}
      </PopoverContent>
    </Popover>
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
  const appName = apps.find((app) => app.id === line.appId)?.name;
  const badge = appName ?? line.sourceId;
  const json = line.raw.trim().startsWith("{");
  let pretty = line.raw;
  if (expanded && json) {
    try {
      pretty = JSON.stringify(JSON.parse(line.raw), null, 2);
    } catch {
      pretty = line.raw;
    }
  }
  const body = expanded ? mask(pretty) : highlight(mask(line.message), query, useRegex);
  return (
    <div
      ref={measureRef}
      data-index={index}
      className="absolute left-0 flex w-full hover:bg-muted/40"
      style={{ transform: `translateY(${start}px)` }}
    >
      <span aria-hidden className={cn("w-1 shrink-0 self-stretch", LEVEL_BAR[line.level] ?? "bg-border")} />
      <div className="flex min-w-0 flex-1 items-start gap-2 px-2 py-1">
        <button
          type="button"
          className="inline-flex size-8 shrink-0 items-center justify-center rounded-md hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none"
          aria-expanded={expanded}
          aria-label={expanded ? "Collapse line" : "Expand line"}
          onClick={onToggle}
        >
          <ChevronRight aria-hidden className={cn("size-4 transition-transform", expanded && "rotate-90")} />
        </button>
        <time className="w-24 shrink-0 pt-1.5 text-muted-foreground" dateTime={new Date(line.tsMs).toISOString()}>
          {new Date(line.tsMs).toLocaleTimeString()}
        </time>
        <span className={cn("w-14 shrink-0 pt-1.5 font-medium uppercase", LEVEL_TEXT[line.level] ?? "")}>
          {line.level}
        </span>
        <span className="mt-1 inline-flex h-6 max-w-36 shrink-0 items-center truncate rounded-md border border-border bg-muted px-1.5 text-[11px] text-foreground">
          {badge}
        </span>
        {appName && appName !== line.sourceId && (
          <span className="w-24 shrink-0 truncate pt-1.5 text-muted-foreground">{line.sourceId}</span>
        )}
        {line.httpStatus ? <span className="w-10 shrink-0 pt-1.5 tabular-nums">{line.httpStatus}</span> : null}
        <span
          className={cn("min-w-0 flex-1 pt-1.5 whitespace-pre-wrap break-all", enabled && "privacy-blur")}
          data-privacy-mask={enabled ? "" : undefined}
        >
          {body}
        </span>
        <Button
          type="button"
          size="icon"
          variant="ghost"
          aria-label="Copy line"
          onClick={() =>
            void navigator.clipboard.writeText(mask(line.raw)).then(
              () => toast.success("Copied line"),
              () => toast.error("Could not copy the line"),
            )
          }
        >
          <Copy />
        </Button>
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

function loadSaved(): SavedFilter[] {
  try {
    return readSavedFilters(localStorage.getItem(SAVED_FILTERS_KEY));
  } catch {
    return [];
  }
}
