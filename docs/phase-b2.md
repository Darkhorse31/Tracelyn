# Phase B2 monitoring

Open **Dashboards** in the sidebar or **Custom dashboards** in the command palette. Choose a starter, duplicate it for a separate copy, add metric, stat, log or Redis panels, and save. Drag panel handles and resize corners. The Left and Down controls also move panels with a keyboard.

Dashboards are version 1 JSON documents in local SQLite. `$server` follows the selected server. Explicit server IDs pin a dashboard. Export downloads JSON and import opens a validated draft with a new ID. Save keeps the draft. The starters are Web app, Node + PM2 and Docker host.

Synthetic workflows live in **Certificates and uptime**. Save a workflow to schedule checks while Tracelyn is open. `vantage` is `pc` or `vps`. Use two workflows to monitor the same service from both locations. VPS checks require an active SSH connection and curl. DNS checks use the PC resolver or VPS getent independently of HTTP availability.

```json
{
  "id": "api-health-pc",
  "serverId": "your-server-id",
  "vantage": "pc",
  "intervalSeconds": 60,
  "enabled": true,
  "steps": [
    {
      "url": "https://example.com/api/version",
      "kind": "http",
      "status": 200,
      "keyword": "version",
      "extract": { "version": "/version" }
    },
    {
      "url": "https://example.com/api/releases/{{version}}",
      "kind": "http",
      "status": 200,
      "keyword": "ready",
      "extract": {}
    }
  ]
}
```

Supported step kinds are `http`, `ssl` and `dns`. HTTP and SSL steps use GET with strict certificate verification. SSL requires HTTPS and curl certificate metadata for the expiry countdown. DNS keyword assertions match resolved addresses. Each workflow supports eight steps, 10 second probe deadlines and up to 500 retained results. Failed assertions stop the workflow. JSON pointers extract variables only after assertions pass. Responses and extracted values are never stored in result history. Saved workflow JSON is local plaintext, so do not put credentials or secrets in it.

Baselines retain eight weeks of minute observations with persisted median and median absolute deviation per server, metric and UTC hour of week. Ribbons use earlier observations only, require 12 observations per seasonal bucket, and flag deviations beyond three scaled MADs with a five percent floor for constant metrics. New installations need history before seasonal comparisons become available.

**What changed** compares a window of up to 24 hours with the same window yesterday. It uses stored metric samples, ingested OpenTelemetry logs and spans, and five minute infrastructure snapshots. Package snapshots support dpkg and RPM. Missing historical snapshots are reported explicitly. Monitoring must have been running in both windows for a complete comparison.

Privacy mode hides workflow definitions, dashboard names and comparison details and disables editing and export. Read-only licenses disable dashboard and workflow writes. Panels use the existing light and dark theme tokens.
