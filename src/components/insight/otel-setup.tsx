import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Notice } from "@/components/status/notice";
import { Masked } from "@/contexts/privacy-context";
import { getInsightSetup, setOtelPaths } from "@/lib/api";
import type { InsightSetup } from "@/lib/insight-types";

const COLLECTOR = `receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 127.0.0.1:4317
      http:
        endpoint: 127.0.0.1:4318
exporters:
  file:
    path: /var/log/otel/telemetry.json
service:
  pipelines:
    traces:
      receivers: [otlp]
      exporters: [file]
    logs:
      receivers: [otlp]
      exporters: [file]
`;

const SNIPPETS: { id: string; label: string; text: string }[] = [
  {
    id: "collector",
    label: "OpenTelemetry Collector",
    text: COLLECTOR,
  },
  {
    id: "node",
    label: "Node.js",
    text: `const { NodeSDK } = require('@opentelemetry/sdk-node');
const { OTLPTraceExporter } = require('@opentelemetry/exporter-trace-otlp-http');
const sdk = new NodeSDK({
  traceExporter: new OTLPTraceExporter({ url: 'http://127.0.0.1:4318/v1/traces' }),
});
sdk.start();
// The Collector file exporter above writes /var/log/otel/telemetry.json`,
  },
  {
    id: "python",
    label: "Python",
    text: `from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk.trace.export import BatchSpanProcessor
provider = TracerProvider()
provider.add_span_processor(BatchSpanProcessor(OTLPSpanExporter(endpoint="http://127.0.0.1:4318/v1/traces")))
# Point Tracelyn at the Collector file: /var/log/otel/telemetry.json`,
  },
  {
    id: "go",
    label: "Go",
    text: `exporter, err := otlptracehttp.New(ctx,
  otlptracehttp.WithEndpoint("127.0.0.1:4318"),
  otlptracehttp.WithInsecure(),
)
// Export to the local Collector. Tracelyn tails its file exporter output.`,
  },
  {
    id: "java",
    label: "Java",
    text: `-Dotel.exporter.otlp.endpoint=http://127.0.0.1:4318
-Dotel.exporter.otlp.protocol=http/protobuf
-Dotel.service.name=my-service`,
  },
  {
    id: "php",
    label: "PHP",
    text: `OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318
OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
OTEL_SERVICE_NAME=my-service`,
  },
  {
    id: "dotnet",
    label: ".NET",
    text: `OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318
OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
OTEL_SERVICE_NAME=my-service`,
  },
];

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
          Nothing is installed on the server. Tracelyn tails these paths over SSH, read-only.
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
      <div className="flex flex-col gap-3">
        <h3 className="text-sm font-medium">How to write the file</h3>
        <p className="text-xs text-muted-foreground">
          Run a Collector with the file exporter, and point each language SDK at it. Tracelyn reads
          the JSON file. It does not open a port on the server.
        </p>
        {SNIPPETS.map((snippet) => (
          <div key={snippet.id} className="rounded-lg border border-border">
            <div className="flex items-center justify-between gap-2 px-3 py-2">
              <h4 className="text-sm font-medium">{snippet.label}</h4>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => {
                  void navigator.clipboard.writeText(snippet.text);
                  toast.success(`Copied ${snippet.label} snippet`);
                }}
              >
                Copy
              </Button>
            </div>
            <pre tabIndex={0} className="max-h-40 overflow-auto border-t border-border bg-muted/40 p-3 font-mono text-xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
              {snippet.text}
            </pre>
          </div>
        ))}
      </div>
    </div>
  );
}
