import { useEffect, useState } from "react";
import { listenWhileMounted } from "@/lib/events";
import { subscribeProcesses, unsubscribeProcesses } from "@/lib/api";
import type { ProcessTreeRow } from "@/lib/types";

export function useProcesses(serverId: string | null, enabled: boolean) {
  const [rows, setRows] = useState<ProcessTreeRow[]>([]);
  const [waiting, setWaiting] = useState(false);

  useEffect(() => {
    if (!serverId || !enabled) {
      setRows([]);
      setWaiting(false);
      return;
    }
    setWaiting(true);
    const stop = listenWhileMounted<ProcessTreeRow[]>(`processes:${serverId}`, (event) => {
      setRows(event.payload);
      setWaiting(false);
    });
    void subscribeProcesses(serverId);
    return () => {
      stop();
      void unsubscribeProcesses(serverId);
    };
  }, [serverId, enabled]);

  return { rows, waiting };
}
