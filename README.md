<div align="center">
  <img src="src/assets/tracelyn-mark.png" alt="Tracelyn Logo" width="80" />
  <h1>Tracelyn</h1>
  <p><strong>Agentless Server Observability, Infrastructure Intelligence, and OpenTelemetry APM Desktop Suite</strong></p>
</div>

---

## Overview

**Tracelyn** is a high-performance, cross-platform desktop observability application built with **Tauri (Rust)** and **React (TypeScript)**. It provides real-time system metrics, automated application discovery, web traffic analytics, Redis introspection, distributed tracing (OpenTelemetry), and safe remote operations over standard SSH connections—without requiring proprietary agents on your target servers.

---

## Key Features

* **System Telemetry & Host Monitoring:** Real-time CPU, memory, swap, load average, disk I/O (`/proc/diskstats`), filesystem usage (`df`), network throughput (`/proc/net/dev`), socket states (`ss`), and process tables (`ps`).
* **Zero-Config Application Discovery:** Automatically maps running services by correlating listening ports (`ss -ltnp`), systemd units, Docker containers, PM2 processes, and Nginx/Apache virtual hosts.
* **Deep Infrastructure Inspection:** Unified visibility into Docker, Kubernetes, PostgreSQL, MySQL, MongoDB, Kafka, RabbitMQ, HAProxy, Traefik, Caddy, Nginx, SSL/TLS certificate expiry, Cron/Systemd timers, Fail2Ban status, and pending OS security updates.
* **OpenTelemetry APM & Distributed Tracing:** Built-in OTLP receiver for ingesting traces, visualizing span waterfalls, generating live service dependency maps, tracking exceptions, and analyzing latency heatmaps/bands.
* **Web Traffic & Access Log Analytics:** Real-time Nginx and Apache access log parsing with route normalization, HTTP status code breakdowns, and latency quantile calculations ($p50$, $p95$, $p99$).
* **Redis Diagnostics:** Native RESP protocol inspector for monitoring memory pressure, keyspace statistics, hit/miss ratios, and slowlog queries.
* **Intelligence & Alerting Engine:** Automated anomaly detection, customizable threshold rules, incident lifecycle tracking, digest reports, and multi-channel notifications.
* **Live Log Explorer:** Virtualized real-time log streaming with log-level filtering and pattern matching.
* **Audited Remote Operations:** Execute quick actions and terminal commands with confirmation guards, operations-mode toggles, and immutable audit trails.

---

## Tech Stack

* **Desktop Core & Backend:** Rust, Tauri v2, Tokio (Async Runtime), SSH2 / OpenSSH Runner, SQLite (Local Metric & Trace Storage)
* **Frontend:** React 18, TypeScript, Vite, Tailwind CSS, shadcn/ui
* **Data Visualization:** uPlot (High-frequency time-series charts), custom SVG Latency Heatmaps & Trace Waterfalls

---

## Project Structure

```text
Tracelyn/
├── docs/                        # Architecture specs, API docs, and UI verification scripts
├── public/                      # Static assets and favicons
├── src/                         # React + TypeScript Frontend
│   ├── assets/                  # Brand marks and icons
│   ├── components/              # Feature components (apm, charts, infra, insight, intel, logs, ops, redis, traces, web, ui)
│   ├── contexts/                # Server, Theme, and Time-Range state providers
│   ├── hooks/                   # Custom React hooks for Tauri IPC commands and event streams
│   ├── lib/                     # TypeScript interfaces, API wrappers, and chart themes
│   └── pages/                   # Application views and dashboards
└── src-tauri/                   # Rust Backend
    ├── capabilities/            # Tauri v2 security permissions
    ├── src/
    │   ├── alerts/              # Threshold rule evaluation and alert history
    │   ├── apps/                # Auto-discovery parsers (Docker, PM2, Nginx, Apache, Systemd, ss)
    │   ├── collectors/          # Linux /proc & CLI collectors + native Redis RESP client
    │   ├── commands/            # Tauri IPC command handlers
    │   ├── config/              # Server profile and app configuration store
    │   ├── connection/          # SSH session manager, Redis probe, and web log pipeline
    │   ├── diagnostics/         # Self-diagnostics and connection health checks
    │   ├── events/              # Real-time frontend event hub and payloads
    │   ├── infra/               # Fleet inspection (DBs, K8s, proxies, certs, cron, capacity)
    │   ├── insight/             # OTLP trace ingest, service map, error aggregation, slowlog
    │   ├── intel/               # Anomaly detection, incident management, digests, notifications
    │   ├── logs/                # Ring-buffer log follower and severity parser
    │   ├── metrics/             # Metric data models and ingestion pipeline
    │   ├── ops/                 # Remote execution planner and audit logger
    │   ├── secrets/             # Secure credential storage
    │   ├── ssh/                 # SSH key handling, host key verification, and sessions
    │   ├── storage/             # Local time-series and trace persistence schema & retention jobs
    │   └── webserver/           # Nginx/Apache log parser, route aggregator, and quantile engine
    └── tests/                   # Backend integration fixtures
