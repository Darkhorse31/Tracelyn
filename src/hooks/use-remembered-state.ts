import { useCallback, useRef, useState, type SetStateAction } from "react";

const sessionPreferences = new Map<string, unknown>();

export function readPreference<T>(key: string, fallback: T): T {
  try {
    if (sessionPreferences.has(key)) return sessionPreferences.get(key) as T;
    const raw = localStorage.getItem(`tracelyn:${key}`);
    if (raw === null) return fallback;
    const value: unknown = JSON.parse(raw);
    if (typeof value === "number" && !Number.isFinite(value)) return fallback;
    if (Array.isArray(fallback))
      return Array.isArray(value) && value.every((v) => typeof v === "string")
        ? (value as T)
        : fallback;
    return typeof value === typeof fallback ? (value as T) : fallback;
  } catch {
    return fallback;
  }
}
export function writePreference(key: string, value: unknown) {
  sessionPreferences.set(key, value);
  try {
    localStorage.setItem(`tracelyn:${key}`, JSON.stringify(value));
  } catch {
    /* Preferences are optional when storage is unavailable. */
  }
}

/** Key changes restore the correct page and server before a filter can be saved. */
export function useRememberedState<T>(
  key: string,
  fallback: T,
): [T, (next: SetStateAction<T>) => void] {
  const initial = useRef(fallback);
  const [entry, setEntry] = useState(() => ({
    key,
    value: readPreference(key, fallback),
  }));
  const value =
    entry.key === key ? entry.value : readPreference(key, initial.current);
  const set = useCallback(
    (next: SetStateAction<T>) => {
      setEntry((previous) => {
        const current =
          previous.key === key
            ? previous.value
            : readPreference(key, initial.current);
        const value =
          typeof next === "function"
            ? (next as (previous: T) => T)(current)
            : next;
        writePreference(key, value);
        return { key, value };
      });
    },
    [key],
  );
  return [value, set];
}
