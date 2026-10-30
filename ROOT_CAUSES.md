# Why traces and logs were empty

Confirmed in this tree. Line numbers are the current source.

## Traces

1. `src-tauri/src/insight/detect.rs:312` and `:324`. Discovery ran `find` and `stat` as the SSH user. `otelcol-contrib` creates `/var/log/otel` mode 0750 and `telemetry.json` mode 0600, so `find` prints nothing and `[ -e file ]` is false when the directory cannot be entered. The file was reported missing, or not listed at all, so nothing was watched. Fix: always probe `/var/log/otel/telemetry.json`, and treat a directory the login cannot enter as unreadable rather than missing. Reads go through `sudo -n`, or `sudo -S` with the in-memory password (`insight/poller.rs:161` and `:278`).

2. `src-tauri/src/insight/poller.rs:847` and `:857`. A large existing file was read from offset 0 with `head -c`, so the capped read stayed on old bytes and never reached new spans. Fix: when the offset is 0 or the file was truncated, start at the last 1 MB and skip to the next newline.

3. `src-tauri/src/insight/otel.rs:192`. The parser accepted camelCase OTLP JSON (`resourceSpans`) and a single span object. Current and older collector lines also use snake_case (`resource_spans`, `trace_id`, `string_value`). Those lines were counted as malformed and dropped. Fix: accept both names. A multi-line `telemetry.json` fixture covers spans, logs, and metrics.

4. `src-tauri/src/insight/store.rs:716`. `adopt_otel_paths` inserts `app_id` as `''`. `watched_files` at `:156` returns those rows. Empty app id was not the drop. The drop was discovery never putting the path in `watch` (item 1). The poller still saves an empty app id and reads the file.

5. `src-tauri/src/connection/manager.rs:320` and `:371`. `insight.detect` runs when the SSH session starts, and `insight.poll` runs on the collector loop. It does not wait for the Traces page.

## Logs

6. `src-tauri/src/logs/follow.rs:66`. `tail -F` ran as the SSH user, so PM2 files under `/root/.pm2/logs` and nginx files failed with permission denied and the hint told the user to run `usermod`. Fix: the same sudo fallback as traces. Discovery (`apps/discover.rs`) reads PM2 `pm_out_log_path` / `pm_err_log_path`, systemd units, Docker, nginx and Apache access and error logs, and adds syslog plus `/var/log/otel/telemetry.json` as server log sources.

## What the user had to paste

7. Setup, traces, logs, web timing, and slow-query screens showed shell to copy. Those screens now use one **Set up monitoring** button. The orchestrator is `src-tauri/src/commands/monitoring.rs` with the step scripts and undo record in `src-tauri/src/setup/wizard.rs`. The administrator password is sent only on `sudo -S` stdin and kept in memory for the connection.
