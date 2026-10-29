import { useCallback, useEffect, useState } from "react";
import { listenWhileMounted } from "@/lib/events";
import type {
  AlertEventRecord,
  AlertRule,
  AlertTransition,
} from "@/lib/alerts-types";
import * as alertsApi from "@/lib/alerts-api";

export function useAlerts(serverId?: string) {
  const [rules, setRules] = useState<AlertRule[]>([]);
  const [history, setHistory] = useState<AlertEventRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(
    async ({ quiet = false }: { quiet?: boolean } = {}) => {
      if (!quiet) setLoading(true);
      setError(null);
      try {
        const [r, h] = await Promise.all([
          alertsApi.listAlertRules(),
          alertsApi.listAlertHistory(serverId),
        ]);
        setRules(r);
        setHistory(h);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    },
    [serverId],
  );

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(
    () =>
      listenWhileMounted<AlertTransition>("alert-transition", () => {
        void refresh({ quiet: true });
      }),
    [refresh],
  );

  const saveRule = useCallback(
    async (rule: AlertRule) => {
      const saved = await alertsApi.upsertAlertRule(rule);
      await refresh({ quiet: true });
      return saved;
    },
    [refresh],
  );

  const removeRule = useCallback(
    async (id: string) => {
      await alertsApi.deleteAlertRule(id);
      await refresh({ quiet: true });
    },
    [refresh],
  );

  const silenceRule = useCallback(
    async (id: string) => {
      await alertsApi.silenceAlertRule(id);
      await refresh({ quiet: true });
    },
    [refresh],
  );

  return {
    rules,
    history,
    loading,
    error,
    refresh,
    saveRule,
    removeRule,
    silenceRule,
  };
}
