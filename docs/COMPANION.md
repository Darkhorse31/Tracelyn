# Server companion

The optional companion runs on Linux under systemd without a public listener. Desktop monitoring remains available without it. Monitoring data stays in `/var/lib/tracelyn`. Configured checks make outbound requests and configured channels receive alert messages. Secrets are transferred over the existing host-key-verified SSH connection and stored in `/etc/tracelyn/channels.json` with owner `tracelyn` and mode 600.

## Build and bundle

The repository root is a Cargo workspace. Windows can check and test the storage, notification, shared parser, and rule code. The service and Unix socket run on Linux.

Install Zig and `cargo-zigbuild`, then run from this worktree:

```powershell
cargo install cargo-zigbuild --locked
./scripts/build-companion.ps1
```

The script builds static musl binaries for x86_64 and aarch64 and places them and SHA-256 sidecars in `src-tauri/resources/companion`. Tauri maps this directory to `companion` in its resources. Missing binaries do not fail desktop builds. Install and Update explain which binary is missing. The GitHub Actions `Companion Linux binaries` workflow builds both targets, runs Linux tests, verifies there is no ELF interpreter, and publishes artifacts. Download artifacts into that resource directory before packaging the desktop application.

On Linux the equivalent build command is:

```sh
rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
cargo zigbuild --release -p tracelyn-companion --target x86_64-unknown-linux-musl
cargo zigbuild --release -p tracelyn-companion --target aarch64-unknown-linux-musl
```

## Desktop setup

Open Server companion on the server page or in Settings. Enable Operations mode and use a writable license. Install opens the existing confirmation dialog with a dry-run command preview. Review it and type `tracelyn-companion` to execute. The desktop uploads files by SFTP, verifies SHA-256, creates the service account and directories, writes the unit and configuration, and enables and starts the service. Every explicit control action is audited. Update replaces the binary and restarts the service.

Installation and control require root or sudo. Save the server sudo password under Operations in Settings to use the existing `sudo -S` flow, which sends the password over SSH on stdin. Without a saved password the app uses `sudo -n`. Verify passwordless access with `sudo -n sh -c true`. Use `sudo visudo` with your administrator to grant appropriate access. Installation needs privileged user creation and file installation, so a simple systemctl-only sudo policy is insufficient. The desktop never embeds a sudo password in the preview, audit log, or config.

For unprivileged history and status access:

```sh
sudo usermod -aG tracelyn YOUR_SSH_USER
```

Reconnect SSH after changing membership. `/run/tracelyn/companion.sock` has mode 660 and group tracelyn. Channel secrets are unreadable by that group. There are no control commands on the socket that execute shell input. The CLI supports `status`, `query`, `events`, and `processes` with JSON output. Query timestamps use Unix milliseconds:

```sh
tracelyn-companion status --json
tracelyn-companion query --since 1793200000000 --until 1793286400000 --metric cpu.percent --json
tracelyn-companion events --since 1793200000000 --json
```

The heartbeat is an atomically replaced Unix-millisecond timestamp in `/var/lib/tracelyn/heartbeat`. Uninstall disables the unit and removes the executable and unit. It preserves history, the service account, and configuration for recovery. Remove those separately if permanently retiring the installation.

## Data, rules, and resource use

CPU, memory, network, disk IO, disk capacity, and load are collected every 5 seconds. The top 20 processes by resident memory are recorded every 30 seconds with CPU counters. SQLite keeps raw metrics and process snapshots for 48 hours and minute averages, minima, and maxima for 30 days. Pruning runs every minute. SQLite reuses freed pages, so the allocated database can retain its high-water size.

The service uses the same shared metric threshold, duration, clear-duration, and hysteresis implementation as desktop intelligence rules. Basic CPU, memory, and disk rules are translated into that format with their existing clearing behavior. Rules and channels sync on save, with failed or disconnected sync retried on reconnect when Operations mode and the license allow writes. Rules are stored in `/etc/tracelyn/rules.json`. Uptime supports TCP or HTTP URL hosts, SSL checks validate certificates, and log patterns read journald. Configure additional log files in the Server companion card, then review and confirm the configuration commands. Paths are saved in `/etc/tracelyn/watch.json` alongside the desktop UTC offset for quiet hours. Ensure the dedicated user can read these files. Desktop-only application and APM rules continue to require their collectors.

History queries use raw samples within the raw retention window and minute summaries for longer ranges. Each request is bounded to 50,000 points. Use `--metric` and a narrower interval for large CLI queries. Desktop queries merge server points with local points, preferring local values at matching timestamps. Server alerts are imported into alert history, incidents, and the change feed with delivery provenance. A recent heartbeat and matching configuration allow the desktop to defer supported rules to the companion. Agentless rule evaluation resumes when the service is stopped or its heartbeat expires.

Telegram, Discord, Slack, ntfy, generic HTTPS webhooks, and SMTP email are delivered directly by the service. SMTP requires TLS or STARTTLS. Outbound requests have timeouts. Metric sampling and socket requests run independently from checks and notification delivery. The systemd memory ceiling is 64 MB, with a 2 MB SQLite cache. Actual CPU and disk use depend on log volume, rule count, and check destinations. Benchmark on your server before enabling many checks.

## Validation

```sh
cargo check --workspace
cargo test --workspace
npm test
npm run build
```

Linux CI also compiles and tests the Unix service. A Windows build does not validate real systemd installation, remote SFTP permissions, journal access, or provider credentials. Exercise install, update, stopped status, reconnect, and offline backfill on a Linux test server before release.
