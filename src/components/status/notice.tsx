import type { ReactNode } from "react";
import { AlertTriangle, CircleAlert, Info, ServerOff } from "lucide-react";
import { cn } from "@/lib/utils";

type Tone = "info" | "warning" | "critical";

const TONES: Record<Tone, { icon: typeof Info; className: string; label: string }> = {
  info: {
    icon: Info,
    className: "border-status-info/40 bg-status-info/10 [&>svg]:text-status-info",
    label: "Note",
  },
  warning: {
    icon: AlertTriangle,
    className: "border-status-warning/40 bg-status-warning/10 [&>svg]:text-status-warning",
    label: "Warning",
  },
  critical: {
    icon: CircleAlert,
    className: "border-status-critical/40 bg-status-critical/10 [&>svg]:text-status-critical",
    label: "Error",
  },
};

/** Inline banner. The icon and a hidden tone label carry the meaning, not color alone. */
export function Notice({
  tone = "info",
  title,
  children,
  action,
  className,
}: {
  tone?: Tone;
  title?: ReactNode;
  children?: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  const { icon: Icon, className: toneClass, label } = TONES[tone];
  return (
    <div
      role={tone === "critical" ? "alert" : "status"}
      className={cn(
        "flex items-start gap-3 rounded-lg border px-4 py-3 text-sm text-foreground",
        toneClass,
        className,
      )}
    >
      <Icon className="mt-0.5 size-4 shrink-0" aria-hidden />
      <div className="min-w-0 flex-1">
        <span className="sr-only">{label}: </span>
        {title && <p className="font-medium">{title}</p>}
        {children && (
          <div className={cn("text-muted-foreground", title && "mt-1")}>{children}</div>
        )}
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}

export function EmptyState({
  title,
  description,
  icon: Icon = ServerOff,
  action,
  className,
}: {
  title: string;
  description?: ReactNode;
  icon?: typeof Info;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border px-6 py-16 text-center",
        className,
      )}
    >
      <div className="flex size-10 items-center justify-center rounded-full bg-muted">
        <Icon className="size-5 text-muted-foreground" aria-hidden />
      </div>
      <h2 className="text-sm font-medium">{title}</h2>
      {description && (
        <p className="max-w-md text-sm text-muted-foreground">{description}</p>
      )}
      {action}
    </div>
  );
}
