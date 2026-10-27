import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { Bell } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { listenWhileMounted } from "@/lib/events";
import {
  ackIncident,
  alertSoundEnabled,
  listIncidents,
  markAlertsRead,
  resolveIncident,
  setAlertSound,
  snoozeIncident,
  unreadAlertCount,
} from "@/pages/intel/api";
import type { Incident } from "@/pages/intel/types";

function beep() {
  const ctx = new AudioContext();
  const osc = ctx.createOscillator();
  const gain = ctx.createGain();
  osc.frequency.value = 880;
  gain.gain.value = 0.04;
  osc.connect(gain);
  gain.connect(ctx.destination);
  osc.start();
  osc.stop(ctx.currentTime + 0.12);
  osc.onended = () => void ctx.close();
}

export function AlertCenter() {
  const navigate = useNavigate();
  const [open, setOpen] = useState(false);
  const [unread, setUnread] = useState(0);
  const [rows, setRows] = useState<Incident[]>([]);
  const [sound, setSound] = useState(true);

  const refresh = () => {
    void unreadAlertCount().then(setUnread).catch(() => undefined);
    void listIncidents().then((next) => setRows(next.filter((row) => row.status !== "resolved").slice(0, 8))).catch(() => undefined);
  };

  useEffect(() => {
    void alertSoundEnabled().then(setSound).catch(() => undefined);
    refresh();
    const timer = setInterval(refresh, 15000);
    return () => clearInterval(timer);
  }, []);

  useEffect(
    () =>
      listenWhileMounted<{ title?: string; path?: string }>("intel-notice", (event) => {
        refresh();
        if (sound) {
          try {
            beep();
          } catch {
            /* audio can be blocked until a click */
          }
        }
        toast.warning(event.payload.title ?? "Tracelyn alert", {
          action: event.payload.path
            ? { label: "Open", onClick: () => navigate(event.payload.path ?? "/alerts") }
            : undefined,
        });
      }),
    [navigate, sound],
  );

  useEffect(
    () =>
      listenWhileMounted<string>("navigate", (event) => {
        if (event.payload) navigate(event.payload);
      }),
    [navigate],
  );

  return (
    <DropdownMenu
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (next) {
          refresh();
          void markAlertsRead().then(() => setUnread(0)).catch(() => undefined);
        }
      }}
    >
      <DropdownMenuTrigger
        className="relative inline-flex size-8 items-center justify-center rounded-lg text-muted-foreground hover:bg-muted hover:text-foreground"
        aria-label={unread > 0 ? `Alerts, ${unread} unread` : "Alerts"}
      >
        <Bell className="size-4" aria-hidden />
        {unread > 0 && (
          <span className="absolute -top-0.5 -right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-status-critical px-1 text-[10px] font-medium text-status-critical-foreground">
            {unread > 9 ? "9+" : unread}
          </span>
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-80 p-2">
        <div className="flex items-center justify-between gap-2 px-1 py-1">
          <p className="text-sm font-medium">Alert center</p>
          <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <input
              type="checkbox"
              checked={sound}
              onChange={(event) => {
                const enabled = event.target.checked;
                setSound(enabled);
                void setAlertSound(enabled);
              }}
            />
            Sound
          </label>
        </div>
        {rows.length === 0 ? (
          <p className="px-1 py-6 text-center text-sm text-muted-foreground">No open alerts. You are all clear.</p>
        ) : (
          <ul className="flex max-h-80 flex-col gap-2 overflow-y-auto">
            {rows.map((row) => (
              <li key={row.id} className="rounded-lg border border-border px-2 py-2">
                <Link to={`/intel/incidents/${row.id}`} className="text-sm font-medium hover:underline" onClick={() => setOpen(false)}>
                  {row.title}
                </Link>
                <p className="text-xs text-muted-foreground">
                  {row.severity} · {row.status}
                  {row.snoozeUntilMs && row.snoozeUntilMs > Date.now() ? " · snoozed" : ""}
                </p>
                <div className="mt-2 flex flex-wrap gap-1">
                  <Button type="button" size="sm" variant="outline" onClick={() => void ackIncident(row.id).then(refresh)}>
                    Ack
                  </Button>
                  <Button type="button" size="sm" variant="outline" onClick={() => void snoozeIncident(row.id, 15).then(refresh)}>
                    15m
                  </Button>
                  <Button type="button" size="sm" variant="outline" onClick={() => void snoozeIncident(row.id, 60).then(refresh)}>
                    1h
                  </Button>
                  <Button type="button" size="sm" variant="outline" onClick={() => void snoozeIncident(row.id, 1440).then(refresh)}>
                    1d
                  </Button>
                  <Button type="button" size="sm" variant="outline" onClick={() => void resolveIncident(row.id).then(refresh)}>
                    Resolve
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
        <Link to="/alerts" className="mt-2 block px-1 text-xs text-muted-foreground underline-offset-4 hover:underline" onClick={() => setOpen(false)}>
          Alert history
        </Link>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
