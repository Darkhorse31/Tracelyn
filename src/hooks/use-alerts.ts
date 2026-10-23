import { useCallback, useEffect, useState } from "react";
import type { AlertEventRecord, AlertRule } from "@/lib/alerts-types";
import * as alertsApi from "@/lib/alerts-api";

export interface AlertsDataSource {
  listRules: () => Promise<AlertRule[]>;
  upsertRule: (rule: AlertRule) => Promise<AlertRule>;
  deleteRule: (id: string) => Promise<void>;
  listHistory: (serverId?: string) => Promise<AlertEventRecord[]>;
}

const MOCK_RULES: AlertRule[] = [
  {
    id: "mock-cpu",
    serverId: null,
    kind: "cpu_high",
    name: "CPU usage high",
    enabled: true,
    params: {
      threshold: 90,
      clearThreshold: 85,
      durationSecs: 300,
      metricKey: "cpu.percent",
    },
    createdAtMs: Date.now() - 86_400_000,
    updatedAtMs: Date.now() - 86_400_000,
  },
  {
    id: "mock-5xx",
    serverId: null,
    kind: "http_5xx_rate",
    name: "HTTP 5xx rate high",
    enabled: true,
    params: {
      threshold: 5,
      clearThreshold: 3,
      durationSecs: 120,
      metricKey: "http.5xx_rate_percent",
    },
    createdAtMs: Date.now() - 86_400_000,
    updatedAtMs: Date.now() - 86_400_000,
  },
];

const MOCK_HISTORY: AlertEventRecord[] = [
  {
    id: "evt-1",
    ruleId: "mock-cpu",
    ruleName: "CPU usage high",
    serverId: "demo-server",
    metricKey: "cpu.percent",
    severity: "critical",
    message: "cpu.percent: 94.2 exceeds threshold 90.0",
    value: 94.2,
    threshold: 90,
    firedAtMs: Date.now() - 3_600_000,
    resolvedAtMs: Date.now() - 3_000_000,
  },
  {
    id: "evt-2",
    ruleId: "mock-5xx",
    ruleName: "HTTP 5xx rate high",
    serverId: "demo-server",
    metricKey: "http.5xx_rate_percent",
    severity: "critical",
    message: "http.5xx_rate_percent: 6.1 exceeds threshold 5.0",
    value: 6.1,
    threshold: 5,
    firedAtMs: Date.now() - 720_000,
    resolvedAtMs: null,
  },
];

const mockSource: AlertsDataSource = {
  async listRules() {
    return MOCK_RULES;
  },
  async upsertRule(rule) {
    const idx = MOCK_RULES.findIndex((r) => r.id === rule.id);
    if (idx >= 0) MOCK_RULES[idx] = rule;
    else MOCK_RULES.push(rule);
    return rule;
  },
  async deleteRule(id) {
    const idx = MOCK_RULES.findIndex((r) => r.id === id);
    if (idx >= 0) MOCK_RULES.splice(idx, 1);
  },
  async listHistory(serverId) {
    if (!serverId) return [...MOCK_HISTORY];
    return MOCK_HISTORY.filter((e) => e.serverId === serverId);
  },
};

/** Set `USE_TAURI_ALERTS` when wiring to the Rust backend. */
const USE_TAURI_ALERTS = true;

function getSource(): AlertsDataSource {
  if (USE_TAURI_ALERTS) {
    return {
      listRules: alertsApi.listAlertRules,
      upsertRule: alertsApi.upsertAlertRule,
      deleteRule: alertsApi.deleteAlertRule,
      listHistory: (serverId) => alertsApi.listAlertHistory(serverId),
    };
  }
  return mockSource;
}

export function useAlerts(serverId?: string) {
  const [rules, setRules] = useState<AlertRule[]>([]);
  const [history, setHistory] = useState<AlertEventRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const source = getSource();
      const [r, h] = await Promise.all([
        source.listRules(),
        source.listHistory(serverId),
      ]);
      setRules(r);
      setHistory(h);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [serverId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const saveRule = useCallback(
    async (rule: AlertRule) => {
      const saved = await getSource().upsertRule(rule);
      await refresh();
      return saved;
    },
    [refresh],
  );

  const removeRule = useCallback(
    async (id: string) => {
      await getSource().deleteRule(id);
      await refresh();
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
  };
}
