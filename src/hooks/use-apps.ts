import { useEffect, useState } from "react";
import {
  listApps,
  subscribeApps,
  unsubscribeApps,
} from "@/lib/api";
import type { TracelynApp } from "@/lib/apps-types";
import { listenThen } from "@/lib/events";
import type { MetricPoint } from "@/lib/types";
import { useServersContext } from "@/contexts/servers-context";

const subscribers = new Map<string, number>();

export interface AppHistory {
  cpu: MetricPoint[];
  mem: MetricPoint[];
  restarts: MetricPoint[];
}

export function useApps() {
  const { selectedServerId } = useServersContext();
  const [apps, setApps] = useState<TracelynApp[]>([]);
  const [history, setHistory] = useState<Record<string, AppHistory>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!selectedServerId) {
      setApps([]);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);
    const apply = (next: TracelynApp[]) => {
      if (cancelled) return;
      setApps(dedupeApps(next));
      setLoading(false);
      const t = Date.now();
      setHistory((prev) => {
        const out = { ...prev };
        for (const app of next) {
          const cur = out[app.id] ?? { cpu: [], mem: [], restarts: [] };
          const push = (points: MetricPoint[], v: number | null | undefined) => {
            if (v == null || Number.isNaN(v)) return points;
            const row = [...points, { t, v }];
            return row.length > 60 ? row.slice(-60) : row;
          };
          out[app.id] = {
            cpu: push(cur.cpu, app.cpuPercent),
            mem: push(cur.mem, app.memoryBytes),
            restarts: push(cur.restarts, app.restarts),
          };
        }
        return out;
      });
    };
    const stop = listenThen<TracelynApp[]>(
      `apps-updated:${selectedServerId}`,
      apply,
      async () => {
        try {
          apply(await listApps(selectedServerId));
          await subscribeApps(selectedServerId);
        } catch (err) {
          if (!cancelled) {
            setError(String(err));
            setLoading(false);
          }
        }
      },
    );
    subscribers.set(selectedServerId, (subscribers.get(selectedServerId) ?? 0) + 1);
    return () => {
      cancelled = true;
      stop();
      const left = (subscribers.get(selectedServerId) ?? 1) - 1;
      if (left <= 0) {
        subscribers.delete(selectedServerId);
        void unsubscribeApps(selectedServerId);
      } else {
        subscribers.set(selectedServerId, left);
      }
    };
  }, [selectedServerId]);

  return { apps, history, loading, error, serverId: selectedServerId };
}

function dedupeApps(apps: TracelynApp[]): TracelynApp[] {
  const out: TracelynApp[] = [];
  for (const app of apps) {
    const prev = out.find(
      (row) =>
        row.id === app.id ||
        (row.kind === "service" && row.kind === app.kind && row.name === app.name),
    );
    if (!prev) {
      out.push({ ...app, domains: [...app.domains], ports: [...app.ports], pids: [...app.pids] });
      continue;
    }
    for (const domain of app.domains) {
      if (!prev.domains.includes(domain)) prev.domains.push(domain);
    }
    for (const port of app.ports) {
      if (!prev.ports.includes(port)) prev.ports.push(port);
    }
    for (const pid of app.pids) {
      if (!prev.pids.includes(pid)) prev.pids.push(pid);
    }
  }
  return out;
}
