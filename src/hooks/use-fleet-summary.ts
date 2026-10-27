import { useCallback, useEffect, useState } from "react";
import { getFleetSummary, type FleetSummary } from "@/lib/fleet-api";
import { listenWhileMounted } from "@/lib/events";

export function useFleetSummary() {
  const [rows, setRows] = useState<FleetSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);
  useEffect(() => {
    let cancelled = false,
      pending = false;
    const load = async () => {
      if (pending) return;
      pending = true;
      try {
        const next = await getFleetSummary();
        if (!cancelled) {
          setRows(next);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) setError(String(err));
      } finally {
        pending = false;
        if (!cancelled) setLoading(false);
      }
    };
    void load();
    const timer = setInterval(() => void load(), 5000);
    const stopStatus = listenWhileMounted("server-statuses", () => void load());
    const stopAlerts = listenWhileMounted(
      "alert-transition",
      () => void load(),
    );
    return () => {
      cancelled = true;
      clearInterval(timer);
      stopStatus();
      stopAlerts();
    };
  }, [revision]);
  return { rows, loading, error, refresh };
}
