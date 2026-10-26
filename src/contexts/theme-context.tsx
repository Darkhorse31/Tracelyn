import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";

export type ThemePreference = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

export const THEME_OPTIONS: { value: ThemePreference; label: string }[] = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "system", label: "System" },
];

const STORAGE_KEY = "tracelyn-theme";
const DARK_QUERY = "(prefers-color-scheme: dark)";
const TRANSITION_MS = 200;

function isPreference(value: unknown): value is ThemePreference {
  return value === "light" || value === "dark" || value === "system";
}

export function readStoredTheme(): ThemePreference {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (isPreference(stored)) return stored;
  } catch {
    // Storage can be unavailable; fall back to the spec's dark default.
  }
  return "dark";
}

function resolve(preference: ThemePreference): ResolvedTheme {
  if (preference !== "system") return preference;
  return window.matchMedia(DARK_QUERY).matches ? "dark" : "light";
}

let transitionTimer: number | undefined;

/** Sets the `dark` class on <html>; safe to call before React mounts. */
export function applyTheme(
  preference: ThemePreference,
  { animate = false }: { animate?: boolean } = {},
): ResolvedTheme {
  const resolved = resolve(preference);
  const root = document.documentElement;
  if (root.classList.contains("dark") === (resolved === "dark")) return resolved;

  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (animate && !reduceMotion) {
    root.classList.add("theme-transition");
    window.clearTimeout(transitionTimer);
    transitionTimer = window.setTimeout(
      () => root.classList.remove("theme-transition"),
      TRANSITION_MS,
    );
  }
  root.classList.toggle("dark", resolved === "dark");
  return resolved;
}

function syncWindowChrome(preference: ThemePreference) {
  if (!("__TAURI_INTERNALS__" in window)) return;
  void getCurrentWindow()
    .setTheme(preference === "system" ? null : preference)
    .catch(() => {});
}

interface ThemeContextValue {
  theme: ThemePreference;
  resolvedTheme: ResolvedTheme;
  setTheme: (theme: ThemePreference) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setThemeState] = useState<ThemePreference>(readStoredTheme);
  const [resolvedTheme, setResolvedTheme] = useState<ResolvedTheme>(() =>
    resolve(theme),
  );

  useEffect(() => {
    syncWindowChrome(theme);
    setResolvedTheme(applyTheme(theme, { animate: true }));
    if (theme !== "system") return;

    const media = window.matchMedia(DARK_QUERY);
    const onChange = () => setResolvedTheme(applyTheme("system", { animate: true }));
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, [theme]);

  const setTheme = useCallback((next: ThemePreference) => {
    try {
      localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Not persisted, but still applied for this session.
    }
    setThemeState(next);
  }, []);

  const value = useMemo(
    () => ({ theme, resolvedTheme, setTheme }),
    [theme, resolvedTheme, setTheme],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
  return ctx;
}
