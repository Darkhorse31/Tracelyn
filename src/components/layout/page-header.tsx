import type { ReactNode } from "react";
import { cn } from "cn";

export function PageHeader({
  eyebrow,
  title,
  description,
  actions,
  mono,
}: {
  eyebrow?: ReactNode;
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  mono?: boolean;
}) {
  return (
    <header className="flex flex-wrap items-end justify-between gap-4">
      <div className="min-w-0">
        {eyebrow ? (
          <p className="mb-2 text-xs font-medium tracking-wide text-muted-foreground uppercase">{eyebrow}</p>
        ) : null}
        <h1 className={cn("text-2xl font-semibold tracking-tight", mono && "font-mono")}>{title}</h1>
        {description ? <div className="mt-2 text-sm text-muted-foreground">{description}</div> : null}
      </div>
      {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
    </header>
  );
}
