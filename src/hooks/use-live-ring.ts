import { useEffect, useState } from "react";
import type { ServerMetricsSnapshot } from "@/lib/types";

export const LIVE_RING_CAP = 600;

export interface LiveSample {
  t: number;
  cpu: number;
  mem: number;
  load: number;
  disk: number;
  diskRead: number;
  diskWrite: number;
  netIn: number;
  netOut: number;
}

export function useLiveRing(snapshot: ServerMetricsSnapshot | null, enabled: boolean) {
  const [samples, setSamples] = useState<LiveSample[]>([]);

  useEffect(() => {
    if (!enabled) {
      setSamples([]);
      return;
    }
    if (!snapshot) return;
    setSamples((prev) => {
      if (prev.length > 0 && prev[prev.length - 1].t === snapshot.timestampMs) return prev;
      const next = prev.concat({
        t: snapshot.timestampMs,
        cpu: snapshot.cpuPercent,
        mem: snapshot.memoryPercent,
        load: snapshot.load1,
        disk: snapshot.diskPercent,
        diskRead: snapshot.diskReadBps,
        diskWrite: snapshot.diskWriteBps,
        netIn: snapshot.netInBps,
        netOut: snapshot.netOutBps,
      });
      return next.length > LIVE_RING_CAP ? next.slice(next.length - LIVE_RING_CAP) : next;
    });
  }, [snapshot, enabled]);

  return samples;
}
