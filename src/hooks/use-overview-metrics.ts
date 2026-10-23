import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import {
  subscribeOverviewMetrics,
  unsubscribeOverviewMetrics,
} from "@/lib/api";
import { useTimeRange } from "@/contexts/time-range-context";
import type { OverviewServerMetrics } from "@/lib/types";

export function useOverviewMetrics(enabled: boolean) {
  const { isLive } = useTimeRange();
  const [byServerId, setByServerId] = useState<
    Map<string, OverviewServerMetrics>
  >(new Map());

  useEffect(() => {
    if (!enabled || !isLive) return;

    let unlisten: UnlistenFn | undefined;

    void (async () => {
      await subscribeOverviewMetrics();
      unlisten = await listen<OverviewServerMetrics[]>(
        "overview-metrics",
        (event) => {
          setByServerId((prev) => {
            const next = new Map(prev);
            for (const row of event.payload) {
              next.set(row.serverId, row);
            }
            return next;
          });
        },
      );
    })();

    return () => {
      void unlisten?.();
      void unsubscribeOverviewMetrics();
    };
  }, [enabled, isLive]);

  return byServerId;
}
