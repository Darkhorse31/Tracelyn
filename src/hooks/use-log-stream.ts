import { useCallback, useEffect, useRef, useState } from "react";
import {
  getLogSnapshot,
  setLogPaused,
  subscribeLogStream,
  unsubscribeLogStream,
} from "@/lib/api";
import { listenThen } from "@/lib/events";
import type { LogLine, LogStreamSnapshot, SourceStatus } from "@/lib/log-types";

const MAX_LINES = 100_000;

export function useLogStream(serverId: string | null, appIds: string[]) {
  const buf = useRef<LogLine[]>([]);
  const pausedRef = useRef(false);
  const [lines, setLines] = useState<LogLine[]>([]);
  const [facets, setFacets] = useState<LogStreamSnapshot["facets"]>({
    level: {},
    appId: {},
  });
  const [sourceStatus, setSourceStatus] = useState<Record<string, SourceStatus>>({});
  const [paused, setPaused] = useState(false);
  const [loading, setLoading] = useState(true);
  const appKey = appIds.slice().sort().join(",");

  const publish = useCallback(() => {
    setLines(buf.current.slice());
  }, []);

  useEffect(() => {
    buf.current = [];
    setLines([]);
    setFacets({ level: {}, appId: {} });
    setSourceStatus({});
    if (!serverId) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    const apply = (snap: LogStreamSnapshot) => {
      if (cancelled) return;
      setFacets(snap.facets);
      setSourceStatus(snap.sourceStatus);
      setLoading(false);
      if (snap.incremental) {
        if (pausedRef.current || snap.lines.length === 0) return;
        buf.current.push(...snap.lines);
        if (buf.current.length > MAX_LINES) {
          buf.current.splice(0, buf.current.length - MAX_LINES);
        }
        publish();
        return;
      }
      buf.current = snap.lines.slice(-MAX_LINES);
      publish();
    };
    const stop = listenThen<LogStreamSnapshot>(
      `log-stream:${serverId}`,
      apply,
      async () => {
        const cached = await getLogSnapshot(serverId);
        if (cached && !cancelled) apply({ ...cached, incremental: false });
        await subscribeLogStream(serverId, appIds.length ? appIds : undefined);
      },
    );
    return () => {
      cancelled = true;
      stop();
      void unsubscribeLogStream(serverId);
    };
    // appIds is represented by appKey
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serverId, appKey, publish]);

  const togglePause = useCallback(async () => {
    const next = !pausedRef.current;
    pausedRef.current = next;
    setPaused(next);
    if (serverId) await setLogPaused(serverId, next);
  }, [serverId]);

  return { lines, facets, sourceStatus, paused, togglePause, loading };
}
