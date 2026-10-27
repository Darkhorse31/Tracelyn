# Track B

The first-run wizard opens after the server list loads successfully with no profiles. It can be skipped, resumed at `/welcome`, or restarted from Settings. It uses the existing connection form and credential storage. Progress stores only the step and selected server, never credentials.

Connection checks validate the profile, probe the SSH port, authenticate and identify the system, and wait for live monitoring to connect. Each check reports its actual result. SSH authentication, discovery, and Setup Guide checks have time limits. Continue later leaves the wizard without losing the saved profile. Setup checks use the same read-only implementation as the Setup Guide. App selections use the existing hidden-app preferences in a single database transaction and trigger rediscovery. The wizard can rediscover previously hidden applications so they can be selected again.

Import is available from the wizard, Settings, the empty dashboard, All servers, and the command palette. It reads `~/.ssh/config`, accepts a chosen file, or accepts pasted text. Previewing never connects to a server. Selectable previews show endpoint duplicates, validation errors, identity files, and ProxyJump. Import checks duplicates again under the configuration lock and saves the batch with one file replacement. Existing profiles and credentials are preserved. Missing credentials can be supplied later in Edit server.

SSH config parsing supports concrete Host aliases, wildcard and negated defaults, first-value precedence, case-insensitive keywords, quoted values, comments, equals separators, HostName, User, Port, IdentityFile, and ProxyJump. Home and common identity-file tokens are expanded. Include files and Match rules are explicitly reported as skipped. The existing SSH transport does not support ProxyJump or ProxyCommand. Such rows are shown with their route and disabled until the source is changed to use a directly reachable host.

CSV requires `host` and `username` columns. `user` is also accepted. Optional columns are `name`, `port`, `authType`, `keyPath`, and `labels`. Labels accept a JSON array or `|` separators. Parsing handles UTF-8 BOM, CRLF, quoted commas, escaped quotes, and multiline fields. Secret columns and malformed quoting are rejected. Export uses a native save dialog and an explicit profile-field allowlist. Exported CSV can be reimported.

All servers is available at `/servers` and becomes the home dashboard with at least two profiles. It supports grid and table layouts, sorting in either direction, search, status and tag filters, grouping by the first tag, and connection, edit, setup, app, and alert navigation. Last seen uses stored samples, including retained rollups. Missing samples are shown as unavailable. Alert counts are read from existing history without changing alert behavior. Filters, sort order, layout, and grouping are remembered.

Ctrl+K opens the command palette. It searches all saved servers, apps currently known to the app cache, navigation pages, and common actions. Arrow keys and Enter work from the focused search input. The ? shortcut opens keyboard help outside text fields and terminals. Existing logs and web-traffic virtualization is retained. Traces now use measured virtual rows, a keyboard-scrollable region, and an optional Show all results mode. Filters are remembered separately for servers and pages across applications, logs, web traffic, traces, API routes, slow queries, infrastructure resources, and audit history.

Shared UI fixes preserve the theme tokens and existing components. Selects display their labels, primary-button hover states maintain text contrast, empty server navigation uses valid list markup, and code snippets can be scrolled with a keyboard. The top bar and fleet cards fit compact windows.

Validation includes Rust parser, CSV, shared-fixture, export, and atomic-import tests, plus TypeScript parsing, duplicate, secret-exclusion, and round-trip tests. `track-b-ui-check.mjs` exercises the wizard, failure and retry, imports, app selection, fleet views, palette, keyboard help, export, filter restoration, and a 2000-row trace list. It mocks Tauri IPC and runs WCAG AA audits in both themes. It does not contact a VPS.

Run the browser checks with a dev server on port 1436 and external Playwright and axe-core packages:

```text
npm run dev -- --port 1436 --host 127.0.0.1
node docs/track-b-ui-check.mjs <verification node_modules>
```

No alert implementation, host-key storage, Tauri configuration, local infrastructure probe, or error boundary was changed. No commits were made.
