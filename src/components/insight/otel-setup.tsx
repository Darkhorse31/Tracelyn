import { useEffect, useState } from "react";
import { toast } from "sonner";
import { MonitoringWizard } from "@/components/setup/monitoring-wizard";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Notice } from "@/components/status/notice";
import { Masked } from "@/contexts/privacy-context";
import { getInsightSetup, setOtelPaths } from "@/lib/api";
import type { InsightSetup } from "@/lib/insight-types";

export function OtelSetup({ serverId, appId }: { serverId: string; appId: string }) {
  const [setup, setSetup] = useState<InsightSetup | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);

  const load = () => {
    void getInsightSetup(serverId)
      .then((next) => {
        setSetup(next);
        setError(null);
      })
      .catch((err) => setError(String(err)));
  };

  useEffect(() => {
    load();
  }, [serverId]);

  const paths = setup?.paths.filter((row) => row.appId === appId).map((row) => row.path) ?? [];
  const suggestions = setup?.suggestions.filter((row) => row.appId === appId || row.appId === "") ?? [];

  const save = async (next: string[]) => {
    setBusy(true);
    try {
      await setOtelPaths(serverId, appId, next);
      toast.success("OpenTelemetry paths saved");
      load();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      {error && (
        <Notice tone="critical" title="Could not load OpenTelemetry setup">
          {error}
        </Notice>
      )}
      {setup && setup.stats.malformed > 0 && (
        <Notice tone="warning" title="Some lines were skipped">
          {setup.stats.malformed} malformed line(s) were not OTLP JSON.
          {setup.stats.lastError ? ` Last read error: ${setup.stats.lastError}` : ""} Confirm the
          Collector file exporter format. Gzip rotations are ignored.
        </Notice>
      )}
      <div>
        <h3 className="text-sm font-medium">Files Tracelyn reads</h3>
        <p className="mt-1 text-xs text-muted-foreground">
          Set up monitoring installs the collector and connects logs. You can also point Tracelyn at a file that is already there.
        </p>
        {paths.length === 0 ? (
          <p className="mt-2 text-sm text-muted-foreground">No file confirmed for this application yet.</p>
        ) : (
          <ul className="mt-2 flex flex-col gap-1">
            {paths.map((item) => (
              <li key={item} className="flex items-center justify-between gap-2 text-sm">
                <span className="truncate font-mono"><Masked text={item} /></span>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  disabled={busy}
                  onClick={() => void save(paths.filter((row) => row !== item))}
                >
                  Remove
                </Button>
              </li>
            ))}
          </ul>
        )}
      </div>
      {suggestions.length > 0 && (
        <div>
          <h3 className="text-sm font-medium">Suggested files</h3>
          <ul className="mt-2 flex flex-col gap-2">
            {suggestions.map((item) => (
              <li key={`${item.appId}-${item.path}`} className="flex items-start justify-between gap-3 text-sm">
                <span>
                  <span className="font-mono"><Masked text={item.path} /></span>
                  <span className="mt-0.5 block text-xs text-muted-foreground">{item.reason}</span>
                </span>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={busy || paths.includes(item.path)}
                  onClick={() => void save([...paths, item.path])}
                >
                  {paths.includes(item.path) ? "Confirmed" : "Confirm"}
                </Button>
              </li>
            ))}
          </ul>
        </div>
      )}
      <form
        className="flex flex-wrap items-end gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (!path.trim()) return;
          void save([...paths, path.trim()]);
          setPath("");
        }}
      >
        <div className="min-w-[16rem] flex-1">
          <Label htmlFor={`otel-path-${appId}`}>Add a file path</Label>
          <Input
            id={`otel-path-${appId}`}
            value={path}
            onChange={(event) => setPath(event.target.value)}
            placeholder="/var/log/otel/telemetry.json"
            className="mt-1 font-mono"
          />
        </div>
        <Button type="submit" disabled={busy || !path.trim()}>
          Add path
        </Button>
      </form>
      <MonitoringWizard serverId={serverId} />
    </div>
  );
}
