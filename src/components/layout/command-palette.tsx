import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  Boxes,
  CircleHelp,
  Command,
  FileUp,
  LayoutDashboard,
  Loader2,
  Search,
  Server,
  Settings,
  Zap,
} from "lucide-react";
import { toast } from "sonner";
import { ServerFormDialog } from "@/components/servers/server-form-dialog";
import { ServerImportDialog } from "@/components/servers/server-import-dialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useServersContext } from "@/contexts/servers-context";
import { Masked, usePrivacy } from "@/contexts/privacy-context";
import { useTheme } from "@/contexts/theme-context";
import { listApps, listSavedSearches, type SavedSearch } from "@/lib/api";
import type { TracelynApp } from "@/lib/apps-types";
import { cn } from "@/lib/utils";

const PAGES = [
  ["Dashboard", "/"],
  ["All servers", "/servers"],
  ["Server metrics", "/server"],
  ["Processes", "/processes"],
  ["Applications", "/apps"],
  ["Logs", "/logs"],
  ["Web traffic", "/web"],
  ["API endpoints", "/apm"],
  ["Traces", "/traces"],
  ["Service map", "/map"],
  ["Errors", "/errors"],
  ["Slow queries", "/slow-queries"],
  ["Redis", "/redis"],
  ["Alerts", "/alerts"],
  ["Intelligence", "/intel"],
  ["Audit", "/ops"],
  ["Terminal", "/ops/terminal"],
  ["Settings", "/settings"],
  ["Setup Guide", "/setup"],
  ["Setup wizard", "/welcome"],
  ["How Tracelyn works", "/how-it-works"],
  ["Databases", "/infra/databases"],
  ["Queues", "/infra/queues"],
  ["Load balancers", "/infra/balancers"],
  ["Containers", "/infra/containers"],
  ["Certificates and uptime", "/infra/certificates"],
  ["Cron", "/infra/cron"],
  ["Capacity", "/infra/capacity"],
  ["Security", "/infra/security"],
];
interface CommandItem {
  id: string;
  label: string;
  detail: string;
  keywords?: string;
  icon: typeof Search;
  run: () => void | Promise<unknown>;
}

export function CommandPalette() {
  const navigate = useNavigate();
  const {
    servers,
    selectedServerId,
    statusById,
    setSelectedServerId,
    connect,
    disconnect,
  } = useServersContext();
  const { toggle } = usePrivacy();
  const { resolvedTheme, setTheme } = useTheme();
  const [open, setOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [addOpen, setAddOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [apps, setApps] = useState<TracelynApp[]>([]);
  const [appsLoading, setAppsLoading] = useState(false);
  const [appsError, setAppsError] = useState(false);
  const [searches, setSearches] = useState<SavedSearch[]>([]);
  const listId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  const selected = servers.find((s) => s.id === selectedServerId);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.isComposing || event.repeat) return;
      if (
        (event.ctrlKey || event.metaKey) &&
        !event.shiftKey &&
        !event.altKey &&
        event.key.toLowerCase() === "k"
      ) {
        event.preventDefault();
        if (helpOpen || addOpen || importOpen) return;
        setOpen((current) => !current);
      } else if (
        event.key === "?" &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.altKey
      ) {
        const target = event.target instanceof Element ? event.target : null;
        if (
          target?.closest(
            "input, textarea, select, [contenteditable=true], [role=combobox], .xterm",
          )
        )
          return;
        if (document.querySelector('[role="dialog"]')) return;
        event.preventDefault();
        setHelpOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [helpOpen, addOpen, importOpen]);
  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActive(0);
    setAppsLoading(true);
    setAppsError(false);
    let cancelled = false;
    void Promise.allSettled(servers.map((server) => listApps(server.id))).then(
      (results) => {
        if (cancelled) return;
        setApps(
          results
            .flatMap((result) =>
              result.status === "fulfilled" ? result.value : [],
            )
            .filter((app) => !app.hidden),
        );
        setAppsError(results.some((result) => result.status === "rejected"));
        setAppsLoading(false);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [open, servers]);

  useEffect(() => {
    if (!open || !selectedServerId) {
      setSearches([]);
      return;
    }
    let cancelled = false;
    void listSavedSearches(selectedServerId)
      .then((rows) => {
        if (!cancelled) setSearches(rows);
      })
      .catch(() => {
        if (!cancelled) setSearches([]);
      });
    return () => {
      cancelled = true;
    };
  }, [open, selectedServerId]);

  const commands: CommandItem[] = useMemo(
    () => [
      ...servers.map((server) => ({
        id: `server:${server.id}`,
        label: server.name,
        detail: "Server",
        keywords: `${server.host} ${server.labels.join(" ")}`,
        icon: Server,
        run: () => {
          setSelectedServerId(server.id);
          navigate("/server");
        },
      })),
      ...apps.map((app) => ({
        id: `app:${app.serverId}:${app.id}`,
        label: app.name,
        detail: `${servers.find((s) => s.id === app.serverId)?.name ?? "Server"} · ${app.kind}`,
        icon: Boxes,
        run: () => {
          setSelectedServerId(app.serverId);
          navigate(`/apps/${encodeURIComponent(app.id)}`);
        },
      })),
      ...searches.map((search) => ({
        id: `search:${search.name}`,
        label: search.name,
        detail: "Saved search",
        keywords: search.query,
        icon: Search,
        run: () => navigate(`/logs?q=${encodeURIComponent(search.query)}`),
      })),
      ...PAGES.map(([label, path]) => ({
        id: `page:${path}`,
        label,
        detail: "Page",
        icon: path === "/settings" ? Settings : LayoutDashboard,
        run: () => navigate(path),
      })),
      {
        id: "action:add",
        label: "Add server",
        detail: "Action",
        icon: Server,
        run: () => setAddOpen(true),
      },
      {
        id: "action:import",
        label: "Import servers from SSH config or CSV",
        detail: "Action",
        icon: FileUp,
        run: () => setImportOpen(true),
      },
      {
        id: "action:theme",
        label: `Switch to ${resolvedTheme === "dark" ? "light" : "dark"} theme`,
        detail: "Action",
        icon: Zap,
        run: () => setTheme(resolvedTheme === "dark" ? "light" : "dark"),
      },
      {
        id: "action:privacy",
        label: "Toggle privacy mode",
        detail: "Action · Ctrl+Shift+P",
        icon: Zap,
        run: toggle,
      },
      {
        id: "action:help",
        label: "Keyboard shortcuts",
        detail: "Help · ?",
        icon: CircleHelp,
        run: () => setHelpOpen(true),
      },
      ...(selected
        ? [
            {
              id: "action:connect",
              label: `${["connected", "degraded", "connecting"].includes(statusById.get(selected.id)?.status ?? "") ? "Disconnect" : "Connect"} ${selected.name}`,
              detail: "Action · Selected server",
              icon: Zap,
              run: () =>
                ["connected", "degraded", "connecting"].includes(
                  statusById.get(selected.id)?.status ?? "",
                )
                  ? disconnect(selected.id)
                  : connect(selected.id),
            },
          ]
        : []),
    ],
    [
      servers,
      apps,
      searches,
      navigate,
      setSelectedServerId,
      resolvedTheme,
      setTheme,
      toggle,
      selected,
      statusById,
      connect,
      disconnect,
    ],
  );
  const filtered = commands.filter((command) =>
    query
      .toLowerCase()
      .trim()
      .split(/\s+/)
      .every((word) =>
        `${command.label} ${command.detail} ${command.keywords ?? ""}`
          .toLowerCase()
          .includes(word),
      ),
  );
  const results = filtered.slice(0, 100);
  const index = Math.min(active, Math.max(0, results.length - 1));
  const execute = (command: CommandItem | undefined) => {
    if (!command) return;
    setOpen(false);
    void Promise.resolve()
      .then(command.run)
      .catch((err) =>
        toast.error("Could not complete action", { description: String(err) }),
      );
  };
  useEffect(() => {
    if (open)
      document
        .getElementById(`${listId}-${index}`)
        ?.scrollIntoView({ block: "nearest" });
  }, [open, index, listId]);

  return (
    <>
      <Button
        variant="outline"
        size="sm"
        onClick={() => setOpen(true)}
        aria-label="Open command palette, Ctrl+K"
      >
        <Search aria-hidden />
        <span className="hidden lg:inline">Search or jump to…</span>
        <kbd className="hidden rounded border border-border px-1.5 py-0.5 text-[10px] text-muted-foreground lg:inline">
          Ctrl K
        </kbd>
      </Button>
      <Button
        variant="ghost"
        size="icon-sm"
        onClick={() => setHelpOpen(true)}
        aria-label="Keyboard shortcuts, question mark"
      >
        <CircleHelp aria-hidden />
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="gap-3 sm:max-w-xl" initialFocus={inputRef}>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Command className="size-4" aria-hidden />
              Jump to anything
            </DialogTitle>
            <DialogDescription>
              Search servers, discovered apps, pages, and actions.
            </DialogDescription>
          </DialogHeader>
          <Input
            ref={inputRef}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setActive(0);
            }}
            role="combobox"
            aria-label="Search commands"
            aria-expanded="true"
            aria-autocomplete="list"
            aria-controls={listId}
            aria-activedescendant={
              results.length ? `${listId}-${index}` : undefined
            }
            autoComplete="off"
            placeholder="Try a server, Logs, or Add server"
            onKeyDown={(event) => {
              if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                event.preventDefault();
                setActive((previous) =>
                  results.length
                    ? (Math.min(previous, results.length - 1) +
                        (event.key === "ArrowDown" ? 1 : -1) +
                        results.length) %
                      results.length
                    : 0,
                );
              } else if (event.key === "Enter") {
                event.preventDefault();
                execute(results[index]);
              } else if (event.key === "Home" && event.ctrlKey) {
                event.preventDefault();
                setActive(0);
              } else if (event.key === "End" && event.ctrlKey) {
                event.preventDefault();
                setActive(Math.max(0, results.length - 1));
              }
            }}
          />
          <div
            role="listbox"
            id={listId}
            aria-label="Matching commands"
            className="max-h-[min(400px,50vh)] space-y-1 overflow-y-auto"
          >
            {results.map((command, i) => (
              <div
                role="option"
                id={`${listId}-${i}`}
                key={command.id}
                aria-selected={i === index}
                className={cn(
                  "flex cursor-pointer items-center gap-3 rounded-lg px-3 py-3",
                  i === index
                    ? "bg-accent text-accent-foreground ring-1 ring-inset ring-border"
                    : "hover:bg-muted",
                )}
                onMouseEnter={() => setActive(i)}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => execute(command)}
              >
                <command.icon className="size-4 shrink-0" aria-hidden />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">
                    <Masked text={command.label} />
                  </p>
                  <p className="truncate text-xs text-muted-foreground">
                    <Masked text={command.detail} />
                  </p>
                </div>
                {i === index && (
                  <kbd className="text-xs text-muted-foreground" aria-hidden>
                    ↵
                  </kbd>
                )}
              </div>
            ))}
          </div>
          <p
            role="status"
            aria-live="polite"
            className="text-xs text-muted-foreground"
          >
            {results.length
              ? `${filtered.length} results${filtered.length > 100 ? ", showing the first 100. Keep typing to narrow the list" : ""}`
              : "No matching commands. Try a page or server name."}
            {appsLoading
              ? " · Loading applications…"
              : appsError
                ? " · Some applications could not be loaded"
                : ""}
          </p>
          <p className="flex items-center gap-2 border-t border-border pt-3 text-xs text-muted-foreground">
            {appsLoading && (
              <Loader2 className="size-3 animate-spin" aria-hidden />
            )}
            ↑ ↓ to move · Enter to open · Esc to close
          </p>
        </DialogContent>
      </Dialog>
      <Dialog open={helpOpen} onOpenChange={setHelpOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Keyboard shortcuts</DialogTitle>
            <DialogDescription>
              Navigate quickly from anywhere in Tracelyn.
            </DialogDescription>
          </DialogHeader>
          <dl className="space-y-4 text-sm">
            {[
              ["Ctrl+K", "Open command palette"],
              ["?", "Show keyboard shortcuts"],
              ["Ctrl+Shift+P", "Toggle privacy mode"],
              ["Ctrl+B", "Toggle sidebar"],
              ["↑ / ↓", "Move through command results"],
              ["Enter", "Open selected command"],
              ["Esc", "Close a dialog"],
              ["Tab / Shift+Tab", "Move between controls"],
            ].map(([keys, description]) => (
              <div
                key={keys}
                className="flex items-center justify-between gap-4"
              >
                <dt>{description}</dt>
                <dd>
                  <kbd className="rounded border border-border bg-muted px-2 py-1 text-xs">
                    {keys}
                  </kbd>
                </dd>
              </div>
            ))}
          </dl>
          <p className="text-xs text-muted-foreground">
            The ? shortcut is inactive while typing. Use Cmd in place of Ctrl
            for the command palette on macOS.
          </p>
        </DialogContent>
      </Dialog>
      <ServerFormDialog open={addOpen} onOpenChange={setAddOpen} />
      <ServerImportDialog open={importOpen} onOpenChange={setImportOpen} />
    </>
  );
}
