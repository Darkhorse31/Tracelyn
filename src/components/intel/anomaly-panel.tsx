import { useEffect, useState } from "react";
import { Download } from "lucide-react";
import { toast } from "sonner";
import { UPlotChart } from "@/components/charts/uplot-chart";
import { EmptyState, Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { SimpleSelect } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { formatTs } from "@/components/intel/format";
import { useServersContext } from "@/contexts/servers-context";
import { useTimeRange } from "@/contexts/time-range-context";
import { anomalySeries, downloadFile, exportReport, listAnomalies } from "@/pages/intel/api";
import { METRIC_KEYS, type AnomalyEvent, type BandPoint } from "@/pages/intel/types";

export function AnomalyPanel() {
  const { selectedServerId, servers } = useServersContext();
  const { range } = useTimeRange();
  const [metric, setMetric] = useState<string>("cpu.percent");
  const [points, setPoints] = useState<BandPoint[] | null>(null);
  const [events, setEvents] = useState<AnomalyEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const server = servers.find((item) => item.id === selectedServerId);

  useEffect(() => {
    if (!selectedServerId) {
      setPoints([]);
      setEvents([]);
      return;
    }
    let cancelled = false;
    setPoints(null);
    Promise.all([
      anomalySeries(selectedServerId, metric, range.startMs, range.endMs),
      listAnomalies(selectedServerId, range.startMs, range.endMs),
    ])
      .then(([series, anomalies]) => {
        if (!cancelled) {
          setPoints(series);
          setEvents(anomalies.filter((item) => item.metricKey === metric));
          setError(null);
        }
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedServerId, metric, range.startMs, range.endMs]);

  const exportSnapshot = async (format: "html" | "pdf") => {
    if (!selectedServerId) return;
    try {
      const file = await exportReport({
        format,
        serverId: selectedServerId,
        metricKey: metric,
        rangeStartMs: range.startMs,
        rangeEndMs: range.endMs,
      });
      downloadFile(file);
    } catch (err) {
      toast.error(String(err));
    }
  };

  return (
    <Card>
      <CardHeader className="flex flex-row items-start justify-between gap-3">
        <div>
          <CardTitle className="text-base">Anomaly bands</CardTitle>
          <CardDescription>
            Each hour-of-week keeps its own EWMA baseline. A point is an anomaly when its robust
            z-score (median absolute deviation) is at least 3.5 after a short warmup.
          </CardDescription>
        </div>
        <div className="flex gap-2">
          <Button type="button" size="sm" variant="outline" disabled={!selectedServerId} onClick={() => exportSnapshot("html")}>
            <Download aria-hidden /> HTML
          </Button>
          <Button type="button" size="sm" variant="outline" disabled={!selectedServerId} onClick={() => exportSnapshot("pdf")}>
            <Download aria-hidden /> PDF
          </Button>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {!selectedServerId && <EmptyState title="No server selected" description="Select a server to see seasonal bands." />}
        {selectedServerId && (
          <div className="grid max-w-sm gap-2">
            <Label htmlFor="anomaly-metric">Metric</Label>
            <SimpleSelect
              id="anomaly-metric"
              value={metric}
              onValueChange={setMetric}
              options={METRIC_KEYS.map((key) => ({ value: key, label: key }))}
            />
          </div>
        )}
        {error && (
          <Notice tone="critical" title="Could not load the series">
            {error}
          </Notice>
        )}
        {selectedServerId && points === null && !error && <Skeleton className="h-56 w-full" />}
        {points && points.length < 2 && selectedServerId && !error && (
          <EmptyState
            title="Not enough samples"
            description={`${server?.name ?? "This server"} has no ${metric} history in the selected time range.`}
          />
        )}
        {points && points.length >= 2 && (
          <UPlotChart
            title={`${metric} with seasonal band`}
            timestamps={points.map((point) => point.tsMs / 1000)}
            formatValue={(value) => value.toFixed(2)}
            height={240}
            series={[
              { label: "Upper", color: "sky", values: points.map((point) => point.high), dash: [2, 3] },
              { label: "Lower", color: "sky", values: points.map((point) => point.low), dash: [2, 3] },
              { label: "Baseline", color: "orange", values: points.map((point) => point.baseline), dash: [6, 4] },
              { label: metric, color: "blue", values: points.map((point) => point.value), fill: true },
            ]}
            bands={[{ upper: 0, lower: 1, color: "sky" }]}
          />
        )}
        {events.length > 0 && (
          <div>
            <h3 className="mb-2 text-sm font-medium">Anomaly events</h3>
            <ul className="flex flex-col gap-1 text-sm">
              {events.slice(0, 12).map((event) => (
                <li key={event.id} className="font-mono text-xs text-muted-foreground">
                  <span className="text-foreground">{formatTs(event.tsMs)}</span> — {event.message}
                </li>
              ))}
            </ul>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
