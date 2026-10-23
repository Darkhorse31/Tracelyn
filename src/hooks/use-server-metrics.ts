import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import {
  subscribeServerMetrics,
  unsubscribeServerMetrics,
} from "@/lib/api";
import type { ServerMetricsSnapshot } from "@/lib/types";

export function useServerMetrics(serverId: string | null, enabled: boolean) {
  const [snapshot, setSnapshot] = useState<ServerMetricsSnapshot | null>(null);
  const [waiting, setWaiting] = useState(false);

  useEffect(() => {
    if (!enabled || !serverId) {
      setSnapshot(null);
      setWaiting(false);
      return;
    }

    let unlisten: UnlistenFn | undefined;
    setWaiting(true);

    void (async () => {
      await subscribeServerMetrics(serverId);
      const eventName = `server-metrics:${serverId}`;
      unlisten = await listen<ServerMetricsSnapshot>(eventName, (event) => {
        setSnapshot(event.payload);
        setWaiting(false);
      });
    })();

    return () => {
      void unlisten?.();
      void unsubscribeServerMetrics(serverId);
    };
  }, [serverId, enabled]);

  return { snapshot, waiting };
}
