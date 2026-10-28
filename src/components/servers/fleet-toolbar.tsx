import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react"
import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  LayoutGrid,
  List,
  Search,
  SlidersHorizontal,
} from "lucide-react"
import { CONNECTION_STATUS_LABELS } from "@/components/status/connection-status-badge"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command"
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import {
  FLEET_SORT_COLUMNS,
  activeFilterCount,
  type FleetFilters,
  type FleetSortKey,
} from "@/lib/fleet-view"
import type { ConnectionStatus } from "@/lib/types"
import { cn } from "cn"

const STATUS_DOT: Record<ConnectionStatus, string> = {
  disconnected: "bg-muted-foreground",
  connecting: "bg-status-info",
  connected: "bg-status-ok",
  degraded: "bg-status-warning",
  offline: "bg-status-critical",
}

const MENU_ITEM =
  "flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-sm outline-none transition-colors duration-150 hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring aria-pressed:bg-muted"

const POPOVER = "duration-150"

function onMenuKeyDown(event: ReactKeyboardEvent<HTMLDivElement>) {
  if (
    event.key !== "ArrowDown" &&
    event.key !== "ArrowUp" &&
    event.key !== "Home" &&
    event.key !== "End"
  )
    return
  const items = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button")]
  if (!items.length) return
  event.preventDefault()
  const current = items.indexOf(document.activeElement as HTMLButtonElement)
  if (event.key === "Home") {
    items[0]?.focus()
    return
  }
  if (event.key === "End") {
    items[items.length - 1]?.focus()
    return
  }
  const delta = event.key === "ArrowDown" ? 1 : -1
  const next = current < 0 ? 0 : (current + delta + items.length) % items.length
  items[next]?.focus()
}

export function FleetToolbar({
  query,
  onQueryChange,
  status,
  onStatusChange,
  counts,
  tags,
  onTagsChange,
  tagOptions,
  sortKey,
  ascending,
  onSortKeyChange,
  onAscendingChange,
  view,
  onViewChange,
  group,
  onGroupChange,
  onClear,
}: {
  query: string
  onQueryChange: (value: string) => void
  status: string
  onStatusChange: (value: string) => void
  counts: Record<ConnectionStatus, number>
  tags: string[]
  onTagsChange: (value: string[]) => void
  tagOptions: string[]
  sortKey: FleetSortKey
  ascending: boolean
  onSortKeyChange: (value: FleetSortKey) => void
  onAscendingChange: (value: boolean) => void
  view: string
  onViewChange: (value: "grid" | "table") => void
  group: boolean
  onGroupChange: (value: boolean) => void
  onClear: () => void
}) {
  const searchRef = useRef<HTMLInputElement>(null)
  const [statusOpen, setStatusOpen] = useState(false)
  const [sortOpen, setSortOpen] = useState(false)
  const filters: FleetFilters = { query, status, tags }
  const count = activeFilterCount(filters)
  const statusName =
    status === "all"
      ? "Status"
      : (CONNECTION_STATUS_LABELS[status as ConnectionStatus] ?? "Status")
  const sortName =
    FLEET_SORT_COLUMNS.find((column) => column.key === sortKey)?.label ?? "Sort"
  const total = Object.values(counts).reduce((sum, value) => sum + value, 0)

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.isComposing || event.repeat) return
      if (
        document.querySelector(
          "[data-slot='dialog-content'][data-open], [data-slot='sheet-content'][data-open]",
        )
      )
        return
      const key = event.key.toLowerCase()
      const command =
        (event.ctrlKey || event.metaKey) &&
        !event.shiftKey &&
        !event.altKey &&
        key === "k"
      const slash =
        event.key === "/" &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.altKey &&
        !event.shiftKey
      if (!command && !slash) return
      if (slash) {
        const target = event.target instanceof Element ? event.target : null
        if (
          target?.closest(
            "input, textarea, select, [contenteditable=true], [role=combobox], .xterm",
          )
        )
          return
      }
      event.preventDefault()
      event.stopImmediatePropagation()
      searchRef.current?.focus()
      searchRef.current?.select()
    }
    window.addEventListener("keydown", onKey, true)
    return () => window.removeEventListener("keydown", onKey, true)
  }, [])

  const toggleTag = (tag: string) => {
    onTagsChange(
      tags.includes(tag) ? tags.filter((item) => item !== tag) : [...tags, tag],
    )
  }

  return (
    <div
      role="toolbar"
      aria-label="Filter and sort servers"
      className="flex flex-wrap items-center gap-2"
    >
      <div className="min-w-48 flex-1 basis-full sm:max-w-md sm:basis-72">
        <InputGroup>
          <InputGroupAddon>
            <Search />
          </InputGroupAddon>
          <InputGroupInput
            ref={searchRef}
            id="fleet-search"
            value={query}
            placeholder="Search servers or tags"
            aria-label="Search servers by name, host, or tag"
            aria-keyshortcuts="Control+K /"
            onChange={(event) => onQueryChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Escape") return
              event.preventDefault()
              event.stopPropagation()
              if (query) onQueryChange("")
              else event.currentTarget.blur()
            }}
          />
          <InputGroupAddon align="inline-end" className="gap-1">
            <kbd aria-hidden className="border border-border bg-muted px-1 text-xs text-muted-foreground">
              Ctrl+K
            </kbd>
            <kbd aria-hidden className="border border-border bg-muted px-1 text-xs text-muted-foreground">
              /
            </kbd>
          </InputGroupAddon>
        </InputGroup>
      </div>

      <Popover open={statusOpen} onOpenChange={setStatusOpen}>
        <PopoverTrigger
          render={
            <Button
              variant={status === "all" ? "outline" : "secondary"}
              aria-label={`Filter by status, ${statusName}`}
              className="max-w-56"
            />
          }
        >
          {status !== "all" && STATUS_DOT[status as ConnectionStatus] && (
            <span
              aria-hidden
              className={cn("size-2 rounded-full", STATUS_DOT[status as ConnectionStatus])}
            />
          )}
          <span className="truncate">{statusName}</span>
          <ChevronDown data-icon="inline-end" />
        </PopoverTrigger>
        <PopoverContent align="start" className={cn("w-60 gap-0 p-1", POPOVER)}>
          <div role="group" aria-label="Status" onKeyDown={onMenuKeyDown} className="flex flex-col gap-0.5">
            <button
              type="button"
              className={MENU_ITEM}
              aria-pressed={status === "all"}
              onClick={() => {
                onStatusChange("all")
                setStatusOpen(false)
              }}
            >
              <span aria-hidden className="size-2 rounded-full bg-foreground/40" />
              <span className="flex-1">All statuses</span>
              <span className="text-xs text-muted-foreground tabular-nums">{total}</span>
            </button>
            {(Object.entries(CONNECTION_STATUS_LABELS) as [ConnectionStatus, string][]).map(
              ([value, label]) => (
                <button
                  key={value}
                  type="button"
                  className={MENU_ITEM}
                  aria-pressed={status === value}
                  onClick={() => {
                    onStatusChange(value)
                    setStatusOpen(false)
                  }}
                >
                  <span aria-hidden className={cn("size-2 rounded-full", STATUS_DOT[value])} />
                  <span className="flex-1">{label}</span>
                  <span className="text-xs text-muted-foreground tabular-nums">
                    {counts[value]}
                  </span>
                </button>
              ),
            )}
          </div>
        </PopoverContent>
      </Popover>

      <Popover>
        <PopoverTrigger
          render={
            <Button
              variant={tags.length ? "secondary" : "outline"}
              aria-label={
                tags.length
                  ? `Filter by tag, ${tags.length} selected`
                  : "Filter by tag"
              }
              className="max-w-56"
            />
          }
        >
          <span className="truncate">
            {tags.length === 0 ? "Tag" : tags.length === 1 ? tags[0] : "Tag"}
          </span>
          {tags.length > 1 && <Badge variant="secondary">{tags.length}</Badge>}
          <ChevronDown data-icon="inline-end" />
        </PopoverTrigger>
        <PopoverContent align="start" className={cn("w-64 gap-0 overflow-hidden p-0", POPOVER)}>
          <Command>
            <CommandInput placeholder="Search tags" aria-label="Search tags" />
            <CommandList>
              <CommandEmpty>No matching tags</CommandEmpty>
              <CommandGroup>
                <CommandItem
                  value="all tags"
                  data-checked={tags.length === 0 ? "true" : undefined}
                  onSelect={() => onTagsChange([])}
                >
                  All tags
                </CommandItem>
                {tagOptions.map((tag) => (
                  <CommandItem
                    key={tag}
                    value={tag}
                    data-checked={tags.includes(tag) ? "true" : undefined}
                    onSelect={() => toggleTag(tag)}
                  >
                    {tag}
                  </CommandItem>
                ))}
              </CommandGroup>
            </CommandList>
          </Command>
        </PopoverContent>
      </Popover>

      <Popover open={sortOpen} onOpenChange={setSortOpen}>
        <PopoverTrigger
          render={
            <Button
              variant="outline"
              aria-label={`Sort by ${sortName}, ${ascending ? "ascending" : "descending"}`}
              className="max-w-56"
            />
          }
        >
          {ascending ? <ArrowUp /> : <ArrowDown />}
          <span className="truncate">{sortName}</span>
          <ChevronDown data-icon="inline-end" />
        </PopoverTrigger>
        <PopoverContent align="start" className={cn("w-56 gap-1 p-1", POPOVER)}>
          <div role="group" aria-label="Sort by" onKeyDown={onMenuKeyDown} className="flex flex-col gap-0.5">
            {FLEET_SORT_COLUMNS.map((column) => (
              <button
                key={column.key}
                type="button"
                className={MENU_ITEM}
                aria-pressed={sortKey === column.key}
                onClick={() => onSortKeyChange(column.key)}
              >
                <span className="flex-1">{column.label}</span>
                {sortKey === column.key &&
                  (ascending ? <ArrowUp aria-hidden /> : <ArrowDown aria-hidden />)}
              </button>
            ))}
          </div>
          <Separator />
          <div role="group" aria-label="Sort direction" className="grid grid-cols-2 gap-1">
            <Button
              type="button"
              size="sm"
              variant={ascending ? "secondary" : "ghost"}
              aria-pressed={ascending}
              onClick={() => onAscendingChange(true)}
            >
              <ArrowUp data-icon="inline-start" />
              Ascending
            </Button>
            <Button
              type="button"
              size="sm"
              variant={ascending ? "ghost" : "secondary"}
              aria-pressed={!ascending}
              onClick={() => onAscendingChange(false)}
            >
              <ArrowDown data-icon="inline-start" />
              Descending
            </Button>
          </div>
        </PopoverContent>
      </Popover>

      <ToggleGroup
        aria-label="Server layout"
        spacing={0}
        variant="outline"
        value={view === "grid" || view === "table" ? [view] : []}
        onValueChange={(next) => {
          const picked = next[0]
          if (picked === "grid" || picked === "table") onViewChange(picked)
        }}
        className="h-8"
      >
        <ToggleGroupItem value="grid" aria-label="Grid view">
          <LayoutGrid />
        </ToggleGroupItem>
        <ToggleGroupItem value="table" aria-label="List view">
          <List />
        </ToggleGroupItem>
      </ToggleGroup>

      <Popover>
        <PopoverTrigger
          render={<Button variant="outline" size="icon" aria-label="View options" />}
        >
          <SlidersHorizontal />
        </PopoverTrigger>
        <PopoverContent align="end" className={cn("w-64 gap-3 p-3", POPOVER)}>
          <p className="text-sm font-medium">View options</p>
          <div className="flex min-h-8 items-center justify-between gap-3">
            <span id="fleet-group-label" className="text-sm">
              Group by first tag
            </span>
            <Switch
              checked={group}
              onCheckedChange={onGroupChange}
              aria-labelledby="fleet-group-label"
            />
          </div>
        </PopoverContent>
      </Popover>

      {count > 0 && (
        <Button
          type="button"
          variant="ghost"
          aria-label={count === 1 ? "Clear 1 filter" : `Clear ${count} filters`}
          onClick={onClear}
        >
          Clear filters
          <Badge variant="secondary">{count}</Badge>
        </Button>
      )}
    </div>
  )
}
