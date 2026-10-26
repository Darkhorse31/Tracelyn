import { useCallback, useEffect, useState } from "react";
import { opsGetMode, opsSetMode } from "@/lib/ops-api";
import { OPS_MODE_EVENT } from "@/lib/ops-types";

export function useOpsMode() {
  const [enabled, setEnabled] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setEnabled(await opsGetMode());
      setError(null);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const onChange = () => {
      void refresh();
    };
    window.addEventListener(OPS_MODE_EVENT, onChange);
    return () => window.removeEventListener(OPS_MODE_EVENT, onChange);
  }, [refresh]);

  const update = useCallback(async (next: boolean) => {
    await opsSetMode(next);
    window.dispatchEvent(new Event(OPS_MODE_EVENT));
  }, []);

  return { enabled, loading, error, refresh, update };
}
