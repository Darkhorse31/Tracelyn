import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { toast } from "sonner";
import { useServersContext } from "@/contexts/servers-context";
import type { AlertTransition } from "@/lib/alerts-types";

/**
 * Surfaces alert fire/clear transitions in-app. Sonner renders toasts inside an
 * `aria-live` region, so screen readers announce them without moving focus.
 */
export function AlertAnnouncer() {
  const { servers } = useServersContext();

  useEffect(() => {
    const names = new Map(servers.map((s) => [s.id, s.name]));
    const unlisten = listen<AlertTransition>("alert-transition", ({ payload }) => {
      const server = names.get(payload.serverId) ?? "Unknown server";
      if (payload.fired) {
        toast.error(`${payload.ruleName} — ${server}`, {
          description: payload.message,
          duration: 10_000,
        });
      } else {
        toast.success(`Resolved: ${payload.ruleName} — ${server}`);
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [servers]);

  return null;
}
