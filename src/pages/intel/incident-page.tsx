import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { toast } from "sonner";
import { Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { formatTs, SeverityBadge, statusLabel } from "@/components/intel/format";
import { useLicense } from "@/contexts/license-context";
import { usePrivacy } from "@/contexts/privacy-context";
import { useServersContext } from "@/contexts/servers-context";
import {
  ackIncident,
  addIncidentNote,
  downloadFile,
  exportReport,
  getIncident,
  resolveIncident,
  snoozeIncident,
} from "@/pages/intel/api";
import type { IncidentDetail } from "@/pages/intel/types";

const KIND_LABEL: Record<string, string> = {
  alert: "Alert",
  deploy: "Deploy",
  anomaly: "Anomaly",
  audit: "Audit",
  note: "Note",
  restart: "Restart",
  error: "Error pattern",
  ops: "Ops action",
  cron: "Cron run",
};

export function IncidentPage() {
  const { incidentId } = useParams();
  const { servers } = useServersContext();
  const { license } = useLicense();
  const { mask } = usePrivacy();
  const canWrite = Boolean(license?.canWrite);
  const [detail, setDetail] = useState<IncidentDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);

  const load = () => {
    if (!incidentId) return;
    getIncident(incidentId)
      .then((next) => {
        setDetail(next);
        setError(null);
      })
      .catch((err) => setError(String(err)));
  };

  useEffect(() => {
    load();
  }, [incidentId]);

  const serverName = servers.find((server) => server.id === detail?.incident.serverId)?.name;

  const act = async (work: () => Promise<void>) => {
    setBusy(true);
    try {
      await work();
      load();
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <Link to="/intel?tab=incidents" className="text-sm text-muted-foreground underline-offset-4 hover:underline">
        Back to incidents
      </Link>
      {error && (
        <Notice tone="critical" title="Could not open this incident">
          {error}
        </Notice>
      )}
      {!detail && !error && <Skeleton className="h-40 w-full" />}
      {detail && (
        <>
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h2 className="text-lg font-semibold tracking-tight">{detail.incident.title}</h2>
              <p className="mt-1 flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
                <SeverityBadge severity={detail.incident.severity} />
                <span>{statusLabel(detail.incident.status)}</span>
                <span>{mask(serverName ?? detail.incident.serverId)}</span>
                <span className="font-mono text-xs">Opened {formatTs(detail.incident.openedAtMs)}</span>
              </p>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={!canWrite || busy || detail.incident.status !== "open"}
                onClick={() => act(() => ackIncident(detail.incident.id))}
              >
                Acknowledge
              </Button>
              <Button
                type="button"
                size="sm"
                disabled={!canWrite || busy || detail.incident.status === "resolved"}
                onClick={() => act(() => resolveIncident(detail.incident.id))}
              >
                Resolve
              </Button>
              {([15, 60, 1440] as const).map((minutes) => (
                <Button
                  key={minutes}
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={!canWrite || busy || detail.incident.status === "resolved"}
                  onClick={() => act(() => snoozeIncident(detail.incident.id, minutes))}
                >
                  {minutes === 15 ? "Snooze 15m" : minutes === 60 ? "Snooze 1h" : "Snooze 1d"}
                </Button>
              ))}
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={async () => {
                  try {
                    downloadFile(await exportReport({ format: "html", incidentId: detail.incident.id }));
                  } catch (err) {
                    toast.error(String(err));
                  }
                }}
              >
                HTML
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={async () => {
                  try {
                    downloadFile(await exportReport({ format: "pdf", incidentId: detail.incident.id }));
                  } catch (err) {
                    toast.error(String(err));
                  }
                }}
              >
                PDF
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={async () => {
                  try {
                    downloadFile(await exportReport({ format: "md", incidentId: detail.incident.id }));
                  } catch (err) {
                    toast.error(String(err));
                  }
                }}
              >
                Markdown
              </Button>
            </div>
          </div>
          <Card>
            <CardHeader>
              <CardTitle className="text-base">Timeline</CardTitle>
              <CardDescription>
                Alert, anomalies, new error patterns, restarts, deploys, and audit actions from 15
                minutes on either side, in time order.
              </CardDescription>
            </CardHeader>
            <CardContent>
              {detail.timeline.length === 0 ? (
                <p className="text-sm text-muted-foreground">No events on this incident yet.</p>
              ) : (
                <ol className="flex flex-col gap-3 border-l border-border pl-4">
                  {detail.timeline.map((entry) => (
                    <li key={`${entry.kind}-${entry.id}`} className="text-sm">
                      <p>
                        <span className="font-medium">{KIND_LABEL[entry.kind] ?? entry.kind}</span>
                        <span className="ml-2 font-mono text-xs text-muted-foreground">{formatTs(entry.tsMs)}</span>
                      </p>
                      <p className="text-muted-foreground">{mask(entry.message)}</p>
                    </li>
                  ))}
                </ol>
              )}
            </CardContent>
          </Card>
          <form
            className="flex flex-col gap-2 sm:flex-row sm:items-end"
            onSubmit={(event) => {
              event.preventDefault();
              const text = note.trim();
              if (!text) return;
              void act(async () => {
                await addIncidentNote(detail.incident.id, text);
                setNote("");
              });
            }}
          >
            <div className="grid flex-1 gap-1.5">
              <Label htmlFor="incident-note">Note</Label>
              <Input id="incident-note" value={note} onChange={(e) => setNote(e.target.value)} maxLength={4000} />
            </div>
            <Button type="submit" disabled={!canWrite || busy || !note.trim()}>
              Add note
            </Button>
          </form>
        </>
      )}
    </div>
  );
}
