import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Share2 } from "lucide-react";
import { EmptyState, Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useServersContext } from "@/contexts/servers-context";
import { useTimeRange } from "@/contexts/time-range-context";
import { getServiceMap } from "@/lib/api";
import type { ServiceMap } from "@/lib/insight-types";

export function ServiceMapPage() {
  const { servers, selectedServerId } = useServersContext();
  const { range, isLive } = useTimeRange();
  const navigate = useNavigate();
  const [map, setMap] = useState<ServiceMap | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<"map" | "table">("map");
  const [hover, setHover] = useState<string | null>(null);
  const server = servers.find((item) => item.id === selectedServerId);

  useEffect(() => {
    if (!selectedServerId) return;
    let cancelled = false;
    const load = () => {
      const end = isLive ? Date.now() : range.endMs;
      const start = isLive ? end - 15 * 60 * 1000 : range.startMs;
      void getServiceMap(selectedServerId, start, end)
        .then((next) => {
          if (!cancelled) {
            setMap(next);
            setError(null);
          }
        })
        .catch((err) => {
          if (!cancelled) setError(String(err));
        });
    };
    load();
    const timer = isLive ? setInterval(load, 5000) : undefined;
    return () => {
      cancelled = true;
      if (timer) clearInterval(timer);
    };
  }, [selectedServerId, isLive, range.startMs, range.endMs]);

  if (!server || !selectedServerId) {
    return <EmptyState icon={Share2} title="No server selected" description="Select a server to see calls between services." />;
  }

  const openService = (name: string) => {
    navigate(`/traces?service=${encodeURIComponent(name)}`);
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold tracking-tight">Service map</h2>
          <p className="text-sm text-muted-foreground">
            {server.name}. Edges come from client, server, producer, and consumer spans.
          </p>
        </div>
        <div role="tablist" aria-label="Service map presentation" className="flex gap-2">
          {(["map", "table"] as const).map((name) => (
            <Button
              key={name}
              type="button"
              role="tab"
              size="sm"
              variant={view === name ? "secondary" : "outline"}
              aria-selected={view === name}
              onClick={() => setView(name)}
            >
              {name === "map" ? "Map" : "Table"}
            </Button>
          ))}
        </div>
      </div>
      {error && (
        <Notice tone="critical" title="Could not load the service map">
          {error}
        </Notice>
      )}
      {!map && !error && <Skeleton className="h-80 w-full" />}
      {map && map.nodes.length === 0 && (
        <EmptyState
          icon={Share2}
          title="No service calls yet"
          description="Traces need client or server spans with a peer service. Confirm an OpenTelemetry file on the Traces page."
        />
      )}
      {map && map.nodes.length > 0 && view === "map" && (
        <MapCanvas map={map} hover={hover} onHover={setHover} onOpen={openService} />
      )}
      {map && map.nodes.length > 0 && view === "table" && (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-sm">
            <caption className="sr-only">Service calls</caption>
            <thead className="text-xs text-muted-foreground">
              <tr className="border-b border-border">
                <th scope="col" className="pb-2 font-medium">From</th>
                <th scope="col" className="pb-2 font-medium">To</th>
                <th scope="col" className="pb-2 text-right font-medium">Rate</th>
                <th scope="col" className="pb-2 text-right font-medium">Errors</th>
                <th scope="col" className="pb-2 text-right font-medium">p95</th>
              </tr>
            </thead>
            <tbody>
              {map.edges.map((edge) => (
                <tr key={`${edge.source}-${edge.target}`} className="border-b border-border/60">
                  <td className="py-2">
                    <button type="button" className="hover:underline" onClick={() => openService(edge.source)}>
                      {edge.source}
                    </button>
                  </td>
                  <td className="py-2">
                    <button type="button" className="hover:underline" onClick={() => openService(edge.target)}>
                      {edge.target}
                    </button>
                  </td>
                  <td className="py-2 text-right tabular-nums">{edge.ratePerSec.toFixed(2)}/s</td>
                  <td className="py-2 text-right tabular-nums">
                    {(edge.errorRate * 100).toFixed(1)}%{edge.errorRate > 0.01 ? " · high" : ""}
                  </td>
                  <td className="py-2 text-right tabular-nums">{edge.p95Ms.toFixed(1)} ms</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="text-xs text-muted-foreground">
        Dashed edges have an error rate above 1%. The table is the keyboard view of the same calls.
        {map?.truncated ? " The map used the newest 20,000 spans in the window." : ""}
      </p>
    </div>
  );
}

function MapCanvas({
  map,
  hover,
  onHover,
  onOpen,
}: {
  map: ServiceMap;
  hover: string | null;
  onHover: (id: string | null) => void;
  onOpen: (id: string) => void;
}) {
  const width = 720;
  const height = 460;
  const cx = width / 2;
  const cy = height / 2;
  const radius = Math.min(width, height) * 0.34;
  const placed = map.nodes.map((node, index) => {
    const angle = (index / Math.max(map.nodes.length, 1)) * Math.PI * 2 - Math.PI / 2;
    return {
      ...node,
      x: map.nodes.length === 1 ? cx : cx + radius * Math.cos(angle),
      y: map.nodes.length === 1 ? cy : cy + radius * Math.sin(angle),
    };
  });
  const at = (label: string) => placed.find((node) => node.label === label);

  return (
    <div className="relative h-[460px] overflow-hidden rounded-lg border border-border bg-card">
      <svg className="absolute inset-0 h-full w-full" viewBox={`0 0 ${width} ${height}`} aria-hidden>
        {map.edges.map((edge) => {
          const source = at(edge.source);
          const target = at(edge.target);
          if (!source || !target) return null;
          const hot = edge.errorRate > 0.01;
          const active = hover === edge.source || hover === edge.target;
          return (
            <line
              key={`${edge.source}-${edge.target}`}
              x1={source.x}
              y1={source.y}
              x2={target.x}
              y2={target.y}
              stroke={hot ? "var(--status-critical)" : "var(--chart-blue)"}
              strokeWidth={active ? 2.5 : 1.25}
              strokeDasharray={hot ? "6 4" : undefined}
            />
          );
        })}
      </svg>
      {placed.map((node) => (
        <button
          key={node.id}
          type="button"
          className="absolute max-w-36 -translate-x-1/2 -translate-y-1/2 truncate rounded-md border border-border bg-background px-2 py-1 text-xs shadow-sm hover:bg-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
          style={{ left: `${(node.x / width) * 100}%`, top: `${(node.y / height) * 100}%` }}
          aria-label={`${node.label}, ${node.spanCount} spans, ${node.errorCount} errors`}
          onMouseEnter={() => onHover(node.label)}
          onMouseLeave={() => onHover(null)}
          onFocus={() => onHover(node.label)}
          onBlur={() => onHover(null)}
          onClick={() => onOpen(node.label)}
        >
          {node.label}
          {node.errorCount > 0 ? ` · ${node.errorCount} errors` : ""}
        </button>
      ))}
    </div>
  );
}
