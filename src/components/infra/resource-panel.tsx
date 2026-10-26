import { useMemo, useState } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { HowToFix } from "@/components/setup/how-to-fix";
import { Notice } from "@/components/status/notice";
import type { Resource, Snapshot } from "./api";
import { cn } from "@/lib/utils";

export interface Column {
  key: string;
  label: string;
  unit?: "percent" | "bytes" | "ms" | "days" | "date" | "rate";
  width?: string;
}
const LABELS: Record<string, string> = {
  loading: "Waiting",
  ok: "OK",
  partial: "Partial",
  not_installed: "Not installed",
  installed_stopped: "Installed, stopped",
  not_configured: "Not configured",
  auth_needed: "Authentication needed",
  permission_denied: "Permission denied",
  timed_out: "Timed out",
  error: "Error",
  critical: "Critical",
  warning: "Warning",
  up: "Up",
  down: "Down",
  unknown: "Unknown",
  exposed: "Wildcard bind",
  not_ready: "Not ready",
};
export function Status({ value }: { value: string }) {
  const lower = value.toLowerCase();
  const good = [
    "ok",
    "up",
    "running",
    "healthy",
    "ready",
    "scheduled",
    "success",
  ].includes(lower);
  const muted = lower === "not_installed";
  const info = lower === "not_configured";
  const bad = [
    "critical",
    "error",
    "down",
    "timed_out",
    "failed",
    "unhealthy",
    "not_ready",
  ].includes(lower);
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 whitespace-nowrap rounded-md border px-2 py-1 text-xs font-medium",
        muted
          ? "border-border bg-muted text-muted-foreground"
          : info
            ? "border-status-info/40 bg-status-info/10 text-foreground"
          : good
          ? "border-status-ok/30 bg-status-ok/10 text-foreground"
          : bad
            ? "border-status-critical/50 bg-status-critical/10 text-foreground"
            : "border-status-warning/40 bg-status-warning/10 text-foreground",
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full",
          muted
            ? "bg-muted-foreground"
            : info
              ? "bg-status-info"
            : good
            ? "bg-status-ok"
            : bad
              ? "bg-status-critical"
              : "bg-status-warning",
        )}
        aria-hidden
      />
      {LABELS[value] ?? value.replace(/_/g, " ")}
    </span>
  );
}
export function bytes(value: number) {
  if (!Number.isFinite(value)) return "Unavailable";
  const units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
  const index = Math.max(
    0,
    Math.min(
      units.length - 1,
      Math.floor(Math.log(Math.max(Math.abs(value), 1)) / Math.log(1024)),
    ),
  );
  return `${(value / 1024 ** index).toLocaleString(undefined, { maximumFractionDigits: 1 })} ${units[index]}`;
}
export function cell(value: unknown, unit?: Column["unit"]): string {
  if (value === null || value === undefined || value === "")
    return "Unavailable";
  if (typeof value === "number") {
    if (!Number.isFinite(value)) return "Unavailable";
    if (unit === "bytes") return bytes(value);
    if (unit === "date") return new Date(value).toLocaleString();
    if (unit === "days")
      return value <= 0 ? "Expired / full" : `${value.toFixed(1)} days`;
    return `${value.toLocaleString(undefined, { maximumFractionDigits: 2 })}${unit === "percent" ? "%" : unit === "ms" ? " ms" : unit === "rate" ? "/s" : ""}`;
  }
  if (Array.isArray(value))
    return value
      .map((entry) => {
        if (typeof entry === "object" && entry !== null && "database" in entry)
          return `${entry.database}: ${bytes(Number(entry.bytes))}`;
        return String(entry);
      })
      .join(", ");
  return typeof value === "object" ? JSON.stringify(value) : String(value);
}
export function ResourcePanel({
  title,
  description,
  snapshot,
  columns,
  children,
}: {
  title: string;
  description?: string;
  snapshot?: Snapshot;
  columns: Column[];
  children?: React.ReactNode;
}) {
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const rows = useMemo(
    () =>
      (snapshot?.rows ?? []).filter((r) =>
        `${r.name} ${r.state} ${Object.values(r.fields)
          .filter((v) => typeof v === "string")
          .join(" ")}`
          .toLowerCase()
          .includes(query.toLowerCase()),
      ),
    [snapshot, query],
  );
  const maxPage = Math.max(0, Math.ceil(rows.length / 25) - 1);
  const current = Math.min(page, maxPage);
  return (
    <Card className="min-w-0">
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <CardTitle>{title}</CardTitle>
          {snapshot && <Status value={snapshot.phase} />}
        </div>
        <CardDescription>{description}</CardDescription>
        {snapshot && (
          <p className="text-xs text-muted-foreground">
            {snapshot.message}
            {snapshot.phase !== "loading" &&
              ` · Collected ${new Date(snapshot.sampledAtMs).toLocaleTimeString()}`}
          </p>
        )}
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {snapshot && snapshot.phase !== "ok" && snapshot.phase !== "not_installed" && (
          <Notice
            tone={
              snapshot.phase === "error" || snapshot.phase === "timed_out"
                ? "critical"
                : snapshot.phase === "not_configured"
                  ? "info"
                  : "warning"
            }
            title={LABELS[snapshot.phase]}
          >
            <span className="flex flex-wrap items-center gap-2">
              <span>
                {snapshot.fixHint ?? "Connect the server and wait for the next sample."}
              </span>
              {(snapshot.fixHint ||
                snapshot.phase === "permission_denied" ||
                snapshot.phase === "auth_needed") && (
                <HowToFix
                  text={`${title} ${snapshot.phase} ${snapshot.fixHint ?? ""} ${snapshot.message}`}
                />
              )}
            </span>
            {snapshot.phase === "error" && snapshot.detail && (
              <details className="mt-2">
                <summary className="cursor-pointer text-sm font-medium">Details</summary>
                <pre className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap rounded-md bg-background/60 p-2 font-mono text-xs">
                  {snapshot.detail}
                </pre>
              </details>
            )}
          </Notice>
        )}
        {children}
        {rows.length > 0 || query ? (
          <>
            <Input
              aria-label={`Search ${title}`}
              placeholder={`Search ${title.toLowerCase()}…`}
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setPage(0);
              }}
              className="max-w-sm"
            />
            <div
              className="overflow-auto"
              tabIndex={0}
              role="region"
              aria-label={`${title} data table`}
            >
              <table className="w-full text-left text-sm">
                <caption className="sr-only">
                  {title}. Unavailable means the source did not report this
                  metric.
                </caption>
                <thead>
                  <tr className="border-b border-border text-xs text-muted-foreground">
                    <th scope="col" className="min-w-48 py-3 pr-4 font-medium">
                      Resource
                    </th>
                    <th scope="col" className="py-3 pr-4 font-medium">
                      State
                    </th>
                    {columns.map((c) => (
                      <th
                        scope="col"
                        key={c.key}
                        className="min-w-28 py-3 pr-4 font-medium"
                      >
                        {c.label}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {rows.slice(current * 25, current * 25 + 25).map((row) => (
                    <ResourceLine key={row.id} row={row} columns={columns} />
                  ))}
                </tbody>
              </table>
            </div>
            {rows.length === 0 && (
              <p className="py-4 text-sm text-muted-foreground">
                No resources match your search.
              </p>
            )}
            <div className="flex items-center justify-between gap-3">
              <p className="text-xs text-muted-foreground">
                {rows.length} resources · Page {current + 1} of {maxPage + 1}
              </p>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  disabled={current === 0}
                  onClick={() => setPage(current - 1)}
                >
                  Previous
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={current >= maxPage}
                  onClick={() => setPage(current + 1)}
                >
                  Next
                </Button>
              </div>
            </div>
          </>
        ) : (
          snapshot?.phase === "ok" && (
            <p className="rounded-md border border-dashed border-border p-6 text-center text-sm text-muted-foreground">
              No resources were reported in this sample. Verify the source
              configuration and read permissions if you expected data.
            </p>
          )
        )}
      </CardContent>
    </Card>
  );
}
function ResourceLine({ row, columns }: { row: Resource; columns: Column[] }) {
  const note = [
    row.fields.note,
    row.fields.slowQueryScope,
    row.fields.confidence,
  ]
    .filter((x) => typeof x === "string")
    .join(" ");
  return (
    <tr className="border-b border-border/60 align-top hover:bg-muted/30">
      <th scope="row" className="max-w-sm py-3 pr-4 font-normal">
        <p className="break-words font-medium">{row.name}</p>
        {note && (
          <p className="mt-1 max-w-sm text-xs font-normal text-muted-foreground">
            {note}
          </p>
        )}
        {Array.isArray(row.fields.limitations) && (
          <p className="mt-1 text-xs text-muted-foreground">
            {row.fields.limitations.join(" · ")}
          </p>
        )}
      </th>
      <td className="py-3 pr-4">
        <Status value={row.state} />
      </td>
      {columns.map((c) => (
        <td
          key={c.key}
          className={cn("max-w-md break-words py-3 pr-4 tabular-nums", c.width)}
        >
          {cell(row.fields[c.key], c.unit)}
        </td>
      ))}
    </tr>
  );
}
