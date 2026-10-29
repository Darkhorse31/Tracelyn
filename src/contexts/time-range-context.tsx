import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  resolveTimeRange,
  type TimeRangePreset,
  type TimeRangeSelection,
} from "@/lib/time-range";

interface TimeRangeContextValue {
  range: TimeRangeSelection;
  setPreset: (preset: TimeRangePreset) => void;
  setCustomRange: (startMs: number, endMs: number) => void;
  isLive: boolean;
  cursorMs: number | null;
  setCursorMs: (ms: number | null) => void;
}

const TimeRangeContext = createContext<TimeRangeContextValue | null>(null);

export function TimeRangeProvider({ children }: { children: ReactNode }) {
  const [range, setRange] = useState<TimeRangeSelection>(() =>
    resolveTimeRange("live"),
  );
  const [cursorMs, setCursorMs] = useState<number | null>(null);

  const setPreset = useCallback((preset: TimeRangePreset) => {
    setRange(resolveTimeRange(preset, Date.now()));
  }, []);

  const setCustomRange = useCallback((startMs: number, endMs: number) => {
    setRange({ preset: "custom", startMs, endMs });
  }, []);

  const value = useMemo(
    () => ({
      range,
      setPreset,
      setCustomRange,
      isLive: range.preset === "live",
      cursorMs,
      setCursorMs,
    }),
    [range, setPreset, setCustomRange, cursorMs],
  );

  return (
    <TimeRangeContext.Provider value={value}>{children}</TimeRangeContext.Provider>
  );
}

export function useTimeRange() {
  const ctx = useContext(TimeRangeContext);
  if (!ctx) {
    throw new Error("useTimeRange must be used within TimeRangeProvider");
  }
  return ctx;
}
