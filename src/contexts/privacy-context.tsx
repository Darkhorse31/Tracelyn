import { Eye, EyeOff } from "lucide-react";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ComponentProps,
  type ReactNode,
} from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useServersContext } from "@/contexts/servers-context";
import { maskField, maskSensitive, type MaskKind, type PrivacyCatalog } from "@/lib/privacy";
import { cn } from "@/lib/utils";

const STORAGE_KEY = "tracelyn-privacy";

interface PrivacyContextValue {
  enabled: boolean;
  setEnabled: (next: boolean) => void;
  toggle: () => void;
  mask: (text: string) => string;
  field: (value: string, kind: MaskKind) => string;
}

const PrivacyContext = createContext<PrivacyContextValue | null>(null);

function readEnabled(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

export function PrivacyProvider({ children }: { children: ReactNode }) {
  const { servers } = useServersContext();
  const [enabled, setEnabledState] = useState(() => {
    const on = readEnabled();
    document.documentElement.dataset.privacy = on ? "on" : "off";
    return on;
  });

  const setEnabled = useCallback((next: boolean) => {
    setEnabledState(next);
    try {
      localStorage.setItem(STORAGE_KEY, next ? "1" : "0");
    } catch {
      // Ignore storage failures. The toggle still applies for this session.
    }
    document.documentElement.dataset.privacy = next ? "on" : "off";
  }, []);

  const toggle = useCallback(() => {
    setEnabled(!enabled);
  }, [enabled, setEnabled]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.shiftKey && !event.altKey && !event.metaKey && event.key.toLowerCase() === "p") {
        event.preventDefault();
        setEnabledState((current) => {
          const next = !current;
          try {
            localStorage.setItem(STORAGE_KEY, next ? "1" : "0");
          } catch {
            // Ignore storage failures.
          }
          document.documentElement.dataset.privacy = next ? "on" : "off";
          return next;
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const catalog = useMemo<PrivacyCatalog>(
    () => ({
      servers: servers.map((server) => ({
        host: server.host,
        username: server.username,
        port: server.port,
        keyPath: server.keyPath,
      })),
    }),
    [servers],
  );

  const mask = useCallback((text: string) => (enabled ? maskSensitive(text, catalog) : text), [enabled, catalog]);
  const field = useCallback(
    (value: string, kind: MaskKind) => (enabled ? maskField(value, catalog, kind) : value),
    [enabled, catalog],
  );

  const value = useMemo(
    () => ({ enabled, setEnabled, toggle, mask, field }),
    [enabled, setEnabled, toggle, mask, field],
  );

  return <PrivacyContext.Provider value={value}>{children}</PrivacyContext.Provider>;
}

export function usePrivacy(): PrivacyContextValue {
  const value = useContext(PrivacyContext);
  if (!value) throw new Error("usePrivacy must be used inside PrivacyProvider");
  return value;
}

export function Masked({ text, className }: { text: string; className?: string }) {
  const { enabled, mask } = usePrivacy();
  if (!enabled) return <span className={className}>{text}</span>;
  return (
    <span className={cn("privacy-blur", className)} data-privacy-mask="">
      {mask(text)}
    </span>
  );
}

export function SecretInput({
  kind,
  value,
  className,
  type,
  placeholder,
  ...props
}: ComponentProps<typeof Input> & { kind: MaskKind; value: string }) {
  const { enabled, field } = usePrivacy();
  return (
    <Input
      {...props}
      type={enabled && type === "number" ? "text" : type}
      value={field(value, kind)}
      readOnly={enabled || props.readOnly}
      placeholder={enabled ? "" : placeholder}
      data-privacy-mask={enabled ? "" : undefined}
      className={cn(className, enabled && "privacy-blur")}
    />
  );
}

export function PrivacyToggle({ className }: { className?: string }) {
  const { enabled, toggle } = usePrivacy();
  const Icon = enabled ? EyeOff : Eye;
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      className={className}
      aria-pressed={enabled}
      aria-keyshortcuts="Control+Shift+P"
      aria-label={enabled ? "Turn privacy mode off" : "Turn privacy mode on"}
      onClick={toggle}
    >
      <Icon data-icon="inline-start" aria-hidden />
      Privacy
    </Button>
  );
}
