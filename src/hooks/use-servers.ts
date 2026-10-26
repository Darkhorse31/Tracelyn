import { useCallback, useEffect, useMemo, useState } from "react";
import { listenWhileMounted } from "@/lib/events";
import {
  connectServer,
  deleteServer,
  disconnectServer,
  listServers,
  listServerStatuses,
  testServerConnection,
  upsertServer,
} from "@/lib/api";
import type {
  ServerRecord,
  ServerStatus,
  TestConnectionInput,
  UpsertServerInput,
} from "@/lib/types";

export function useServers() {
  const [servers, setServers] = useState<ServerRecord[]>([]);
  const [statuses, setStatuses] = useState<ServerStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedServerId, setSelectedServerId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const [nextServers, nextStatuses] = await Promise.all([
      listServers(),
      listServerStatuses(),
    ]);
    setServers(nextServers);
    setStatuses(nextStatuses);
    if (!selectedServerId && nextServers.length > 0) {
      setSelectedServerId(nextServers[0].id);
    }
  }, [selectedServerId]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        await refresh();
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [refresh]);

  useEffect(
    () =>
      listenWhileMounted<ServerStatus[]>("server-statuses", (event) => {
        setStatuses(event.payload);
      }),
    [],
  );

  const statusById = useMemo(() => {
    const map = new Map<string, ServerStatus>();
    for (const s of statuses) map.set(s.id, s);
    return map;
  }, [statuses]);

  const saveServer = useCallback(
    async (input: UpsertServerInput) => {
      const saved = await upsertServer(input);
      await refresh();
      setSelectedServerId(saved.id);
      return saved;
    },
    [refresh],
  );

  const removeServer = useCallback(
    async (id: string) => {
      await deleteServer(id);
      if (selectedServerId === id) setSelectedServerId(null);
      await refresh();
    },
    [refresh, selectedServerId],
  );

  const testConnection = useCallback(async (input: TestConnectionInput) => {
    return testServerConnection(input);
  }, []);

  const connect = useCallback(
    async (id: string) => {
      const status = await connectServer(id);
      await refresh();
      return status;
    },
    [refresh],
  );

  const disconnect = useCallback(
    async (id: string) => {
      const status = await disconnectServer(id);
      await refresh();
      return status;
    },
    [refresh],
  );

  return {
    servers,
    statuses,
    statusById,
    loading,
    selectedServerId,
    setSelectedServerId,
    refresh,
    saveServer,
    removeServer,
    testConnection,
    connect,
    disconnect,
  };
}

export type ServersContextValue = ReturnType<typeof useServers>;
