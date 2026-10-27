import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { TerminalSquare } from "lucide-react";
import { toast } from "sonner";
import { OpsModeBanner } from "@/components/ops/operations-mode-card";
import { EmptyState, Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { usePrivacy } from "@/contexts/privacy-context";
import { useServersContext } from "@/contexts/servers-context";
import { useTheme } from "@/contexts/theme-context";
import { listenWhileMounted } from "@/lib/events";
import { opsTerminalClose, opsTerminalOpen, opsTerminalResize, opsTerminalWrite } from "@/lib/ops-api";
import type { TermEvent } from "@/lib/ops-types";
import { extractLastOutput, stripAnsi } from "@/lib/terminal-text";
import { useOpsMode } from "@/hooks/use-ops";
import { cn } from "@/lib/utils";

const openedKeys = new Set<string>();
const pendingClose = new Map<string, number>();

function cssColor(variable: string, fallback: string): string {
  const probe = document.createElement("div");
  probe.style.background = `var(${variable})`;
  probe.style.position = "absolute";
  probe.style.left = "-9999px";
  document.body.appendChild(probe);
  const color = getComputedStyle(probe).backgroundColor;
  probe.remove();
  if (!color || color === "rgba(0, 0, 0, 0)") return fallback;
  return color;
}

function decodeB64(value: string): Uint8Array {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

function bufferText(term: Terminal): string {
  const buffer = term.buffer.active;
  const lines: string[] = [];
  for (let i = 0; i < buffer.length; i += 1) {
    lines.push(buffer.getLine(i)?.translateToString(true) ?? "");
  }
  return lines.join("\n").replace(/\n+$/, "");
}

function saveText(filename: string, text: string) {
  const blob = new Blob([text], { type: "text/plain" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

function TerminalPane({
  tabId,
  active,
  onClosed,
}: {
  tabId: string;
  active: boolean;
  onClosed: () => void;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const { resolvedTheme } = useTheme();
  const { enabled, mask } = usePrivacy();
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const rawRef = useRef("");
  const enabledRef = useRef(enabled);
  enabledRef.current = enabled;
  const maskRef = useRef(mask);
  maskRef.current = mask;
  const activeRef = useRef(active);
  activeRef.current = active;
  const onClosedRef = useRef(onClosed);
  onClosedRef.current = onClosed;
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const copyItemRef = useRef<HTMLButtonElement>(null);

  const show = (text: string) => (enabledRef.current ? maskRef.current(stripAnsi(text)) : text);

  const copyText = async (text: string, message: string) => {
    const value = maskRef.current(text);
    if (!value) {
      toast.message("Nothing selected");
      return;
    }
    await navigator.clipboard.writeText(value);
    toast.success(message);
  };

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const queued = pendingClose.get(tabId);
    if (queued) {
      window.clearTimeout(queued);
      pendingClose.delete(tabId);
    }
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const term = new Terminal({
      cursorBlink: !reduceMotion,
      fontFamily: '"JetBrains Mono", ui-monospace, monospace',
      fontSize: 13,
      scrollback: 2000,
      theme: {
        background: cssColor("--background", "#0b1220"),
        foreground: cssColor("--foreground", "#e5e7eb"),
        cursor: cssColor("--primary", "#3b82f6"),
        selectionBackground: cssColor("--accent", "#334155"),
      },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    fitRef.current = fit;
    fit.fit();
    termRef.current = term;
    void opsTerminalResize(tabId, term.cols, term.rows).catch(() => {});

    term.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown" || !event.ctrlKey) return true;
      const key = event.key.toLowerCase();
      if (event.shiftKey && key === "c") {
        void copyText(term.getSelection(), "Selection copied");
        return false;
      }
      if (event.shiftKey && key === "v") {
        void navigator.clipboard.readText().then((text) => term.paste(text));
        return false;
      }
      if (event.shiftKey && key === "p") return false;
      if (!event.shiftKey && !event.altKey && !event.metaKey && key === "c" && term.hasSelection()) {
        void copyText(term.getSelection(), "Selection copied");
        return false;
      }
      return true;
    });

    const onData = term.onData((data) => {
      if (data.includes("\r")) rawRef.current = `${rawRef.current}\r`.slice(-400_000);
      void opsTerminalWrite(tabId, data).catch(() => {});
    });
    const onResize = term.onResize(({ cols, rows }) => {
      void opsTerminalResize(tabId, cols, rows).catch(() => {});
    });
    const observer = new ResizeObserver(() => {
      if (activeRef.current) fit.fit();
    });
    observer.observe(host);
    const onContext = (event: MouseEvent) => {
      event.preventDefault();
      const x = Math.min(event.clientX, window.innerWidth - 160);
      const y = Math.min(event.clientY, window.innerHeight - 88);
      setMenu({ x, y });
    };
    const onMouseUp = () => {
      const selection = term.getSelection();
      if (selection) void copyText(selection, "Selection copied");
    };
    host.addEventListener("contextmenu", onContext);
    host.addEventListener("mouseup", onMouseUp);

    const stop = listenWhileMounted<TermEvent>(`ops-terminal:${tabId}`, (event) => {
      if (event.payload.dataB64) {
        const bytes = decodeB64(event.payload.dataB64);
        const text = new TextDecoder().decode(bytes);
        rawRef.current = `${rawRef.current}${text}`.slice(-400_000);
        term.write(show(text));
      }
      if (event.payload.closed) onClosedRef.current();
    });

    return () => {
      stop();
      onData.dispose();
      onResize.dispose();
      observer.disconnect();
      host.removeEventListener("contextmenu", onContext);
      host.removeEventListener("mouseup", onMouseUp);
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
      const timer = window.setTimeout(() => {
        pendingClose.delete(tabId);
        void opsTerminalClose(tabId);
      }, 50);
      pendingClose.set(tabId, timer);
    };
    // Theme and visibility are applied below so the PTY is not reopened.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabId]);

  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.theme = {
      background: cssColor("--background", "#0b1220"),
      foreground: cssColor("--foreground", "#e5e7eb"),
      cursor: cssColor("--primary", "#3b82f6"),
      selectionBackground: cssColor("--accent", "#334155"),
    };
    if (active) {
      fitRef.current?.fit();
      term.focus();
    }
  }, [resolvedTheme, active]);

  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.reset();
    term.write(show(rawRef.current));
  }, [enabled]);

  useEffect(() => {
    if (!menu) return;
    copyItemRef.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenu(null);
    };
    const onPointer = (event: MouseEvent) => {
      const target = event.target;
      if (target instanceof Element && target.closest("[role='menu']")) return;
      setMenu(null);
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onPointer);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onPointer);
    };
  }, [menu]);

  const copySelection = () => {
    const term = termRef.current;
    void copyText(term?.getSelection() ?? "", "Selection copied");
  };
  const copyLast = () => {
    void copyText(extractLastOutput(rawRef.current), "Last output copied");
  };
  const copyAll = () => {
    const term = termRef.current;
    void copyText(term ? bufferText(term) : stripAnsi(rawRef.current), "Terminal copied");
  };
  const saveAll = () => {
    const term = termRef.current;
    const text = mask(term ? bufferText(term) : stripAnsi(rawRef.current));
    saveText("terminal.txt", text);
    toast.success("Terminal saved");
  };

  return (
    <div hidden={!active} className={cn(!active && "hidden")}>
      <div role="toolbar" aria-label="Terminal copy" className="mb-2 flex flex-wrap gap-2">
        <Button type="button" size="sm" variant="outline" onClick={copySelection} aria-label="Copy selection">
          Copy selection
        </Button>
        <Button type="button" size="sm" variant="outline" onClick={copyLast} aria-label="Copy last output">
          Copy last output
        </Button>
        <Button type="button" size="sm" variant="outline" onClick={copyAll} aria-label="Copy all">
          Copy all
        </Button>
        <Button type="button" size="sm" variant="outline" onClick={saveAll} aria-label="Save as text file">
          Save as .txt
        </Button>
      </div>
      <div
        ref={hostRef}
        role="region"
        aria-label="SSH terminal"
        className={cn(
          "h-[min(70vh,720px)] overflow-hidden rounded-lg border border-border bg-background p-2",
          enabled && "privacy-blur",
        )}
      />
      {menu && (
        <div
          role="menu"
          aria-label="Terminal clipboard"
          className="fixed z-50 min-w-36 rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md"
          style={{ left: menu.x, top: menu.y }}
        >
          <button
            ref={copyItemRef}
            type="button"
            role="menuitem"
            className="block w-full rounded-sm px-2 py-1.5 text-left text-sm hover:bg-muted"
            onClick={() => {
              copySelection();
              setMenu(null);
            }}
          >
            Copy
          </button>
          <button
            type="button"
            role="menuitem"
            className="block w-full rounded-sm px-2 py-1.5 text-left text-sm hover:bg-muted"
            onClick={() => {
              const term = termRef.current;
              if (term) {
                void navigator.clipboard.readText().then((text) => term.paste(text));
              }
              setMenu(null);
            }}
          >
            Paste
          </button>
        </div>
      )}
    </div>
  );
}

export function TerminalPage() {
  const { servers, selectedServerId } = useServersContext();
  const server = servers.find((row) => row.id === selectedServerId);
  const { enabled, loading } = useOpsMode();
  const [params] = useSearchParams();
  const [tabs, setTabs] = useState<{ id: string; title: string }[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const seq = useRef(1);
  const serverId = server?.id;
  const serverName = server?.name;

  const openTab = async () => {
    if (!serverId || !serverName || !enabled) return;
    setOpening(true);
    setError(null);
    try {
      const { tabId } = await opsTerminalOpen(serverId, 80, 24);
      const title = `${serverName} #${seq.current}`;
      seq.current += 1;
      setTabs((current) => [...current, { id: tabId, title }]);
      setActive(tabId);
    } catch (err) {
      setError(String(err));
    } finally {
      setOpening(false);
    }
  };

  const autoKey = params.get("new");
  useEffect(() => {
    if (!autoKey || !enabled || !serverId || openedKeys.has(autoKey)) return;
    openedKeys.add(autoKey);
    void openTab();
    // openTab identity changes with server; the key gates a single open.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoKey, enabled, serverId]);

  if (!server) {
    return (
      <EmptyState
        icon={TerminalSquare}
        title="No server selected"
        description="Select a server, turn on Operations mode, then open a shell."
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold tracking-tight">Terminal</h2>
          <p className="text-sm text-muted-foreground">
            {server.name}. Ctrl+Shift+C copies, Ctrl+Shift+V or right-click pastes. Ctrl+C still
            interrupts the remote process.
          </p>
        </div>
        <Button type="button" onClick={() => void openTab()} disabled={!enabled || loading || opening || tabs.length >= 8}>
          New tab
        </Button>
      </div>
      <OpsModeBanner />
      {error && (
        <Notice tone="critical" title="Could not open a shell">
          {error}
        </Notice>
      )}
      {tabs.length === 0 ? (
        <EmptyState
          icon={TerminalSquare}
          title="No terminal open"
          description={
            enabled
              ? "Open a tab to start a shell on the selected server. Up to 8 tabs."
              : "Turn on Operations mode in Settings to open a shell."
          }
        />
      ) : (
        <>
          <div role="tablist" aria-label="Terminal tabs" className="flex flex-wrap gap-2">
            {tabs.map((tab) => (
              <div key={tab.id} className="flex items-center gap-1">
                <button
                  type="button"
                  role="tab"
                  aria-selected={tab.id === active}
                  className={cn(
                    "rounded-md px-3 py-1.5 text-sm",
                    tab.id === active ? "bg-muted font-medium" : "text-muted-foreground hover:bg-muted/60",
                  )}
                  onClick={() => setActive(tab.id)}
                >
                  {tab.title}
                </button>
                <Button
                  type="button"
                  size="xs"
                  variant="ghost"
                  aria-label={`Close ${tab.title}`}
                  onClick={() => {
                    setTabs((current) => current.filter((row) => row.id !== tab.id));
                    setActive((current) => (current === tab.id ? null : current));
                    void opsTerminalClose(tab.id);
                  }}
                >
                  Close
                </Button>
              </div>
            ))}
          </div>
          {tabs.map((tab) => (
            <TerminalPane
              key={tab.id}
              tabId={tab.id}
              active={tab.id === active}
              onClosed={() => {
                setTabs((current) => current.filter((row) => row.id !== tab.id));
                setActive((current) => (current === tab.id ? null : current));
              }}
            />
          ))}
        </>
      )}
    </div>
  );
}
