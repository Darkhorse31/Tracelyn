// Run: node docs/track-b-ui-check.mjs <verification node_modules> [base URL]
// Uses existing Playwright and axe-core packages outside the app. All IPC is mocked.
import { createRequire } from "node:module";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";

const require = createRequire(path.resolve(process.argv[2], "package.json"));
const { chromium } = require("playwright");
const axePath = require.resolve("axe-core/axe.min.js");
const base = process.argv[3] || "http://127.0.0.1:1436";
const output = path.resolve(process.argv[2], "..", "track-b-results");
await fs.mkdir(output, { recursive: true });
const browser = await chromium.launch({
  executablePath:
    "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  headless: true,
});
const errors = [],
  audits = [];

async function audit(page, name) {
  await page.addScriptTag({ path: axePath });
  const result = await page.evaluate(() =>
    axe.run(document, {
      runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21aa"] },
    }),
  );
  const violations = result.violations.map((v) => ({
    id: v.id,
    impact: v.impact,
    nodes: v.nodes.map((n) => ({
      target: n.target,
      summary: n.failureSummary,
    })),
  }));
  audits.push({ name, violations });
  await page.screenshot({
    path: path.join(output, `${name}.png`),
    fullPage: true,
  });
}
async function jump(page, query) {
  await page.keyboard.press("Control+k");
  await page.getByRole("combobox", { name: "Search commands" }).fill(query);
  await page.getByRole("combobox", { name: "Search commands" }).press("Enter");
}

try {
  for (const theme of ["light", "dark"]) {
    const context = await browser.newContext({
      viewport: { width: 1360, height: 1000 },
      reducedMotion: "reduce",
    });
    await context.addInitScript(
      ({ theme }) => {
        localStorage.setItem("tracelyn-theme", theme);
        let callback = 0;
        const profiles = [];
        const calls = [];
        window.__trackB = { profiles, calls, failConnection: false };
        const app = (serverId, n) => ({
          id: `app-${n}`,
          serverId,
          name: n === 1 ? "Orders API" : "Worker",
          kind: "systemd",
          status: "online",
          origin: "discovered",
          hidden: false,
          pids: [100 + n],
          ports: [3000 + n],
          domains: [],
          tags: [],
          logSources: [],
          linkedAppIds: [],
        });
        window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
        window.__TAURI_INTERNALS__ = {
          metadata: {
            currentWindow: { label: "main" },
            currentWebview: { label: "main" },
          },
          transformCallback: () => ++callback,
          unregisterCallback() {},
          invoke: async (cmd, args = {}) => {
            calls.push({ cmd, args });
            if (cmd === "list_servers") return structuredClone(profiles);
            if (cmd === "list_server_statuses")
              return profiles.map((p) => ({
                id: p.id,
                status: "connected",
                hostname: p.host,
                osInfo: "Linux",
                lastError: null,
              }));
            if (cmd === "import_server_profiles") {
              const imported = args.inputs.map((p, i) => ({
                ...p,
                id: `server-${profiles.length + i + 1}`,
              }));
              profiles.push(...imported);
              return { imported, skipped: 0 };
            }
            if (cmd === "upsert_server") {
              const saved = {
                ...args.input,
                id: args.input.id || `server-${profiles.length + 1}`,
              };
              const index = profiles.findIndex((p) => p.id === saved.id);
              if (index < 0) profiles.push(saved);
              else profiles[index] = saved;
              return saved;
            }
            if (
              cmd === "probe_server_port" ||
              cmd === "test_onboarding_connection" ||
              cmd === "start_onboarding_monitoring"
            ) {
              await new Promise((resolve) => setTimeout(resolve, 80));
              if (window.__trackB.failConnection && cmd === "probe_server_port")
                throw Error("Connection timed out. Check your VPN");
              return {
                success: true,
                osInfo: "Linux",
                hostname: "vps.example.com",
              };
            }
            if (
              cmd === "run_setup_checks" ||
              cmd === "run_onboarding_setup_checks"
            )
              return [
                {
                  id: "ssh",
                  title: "SSH connection",
                  status: "pass",
                  explanation: "SSH works",
                  details: [],
                },
                {
                  id: "logs",
                  title: "Application logs",
                  status: "warning",
                  explanation: "Review log access",
                  details: ["Some log files need read access"],
                  command: "id deploy",
                },
              ];
            if (cmd === "discover_onboarding_apps" || cmd === "list_apps")
              return [app(args.serverId, 1), app(args.serverId, 2)];
            if (cmd === "get_fleet_summary")
              return profiles.map((p, i) => ({
                serverId: p.id,
                appCount: 2,
                activeAlertCount: i + 1,
                lastSeenMs: Date.now() - i * 60000,
                cpuPercent: 10 + i * 20,
                memoryPercent: 50 + i * 10,
                diskPercent: 42 + i * 5,
              }));
            if (cmd === "list_ssh_private_keys")
              return { directory: "C:/keys", keys: ["C:/keys/id_ed25519"] };
            if (cmd === "inspect_ssh_private_key")
              return { ok: true, message: "Valid key" };
            if (cmd === "plugin:dialog|save") return "C:/fixture/servers.csv";
            if (cmd === "get_ops_mode" || cmd === "ops_get_mode")
              return { enabled: false, servers: {} };
            if (cmd === "get_web_traffic_snapshot")
              return {
                serverId: args.serverId,
                metrics: {
                  reqPerSec: 0,
                  bandwidthMbps: 0,
                  statusBreakdown: {},
                  topUrls: [],
                  topIps: [],
                  reqPerSecSeries: [],
                },
                logs: [],
                permissionDenied: false,
                webDetected: true,
              };
            if (cmd === "get_log_snapshot")
              return {
                serverId: args.serverId,
                lines: [],
                facets: { level: {}, appId: {} },
                sourceStatus: {},
                incremental: false,
              };
            if (cmd === "list_traces")
              return Array.from({ length: 2000 }, (_, i) => ({
                traceId: `trace-${i}`,
                service: "orders",
                operation: `GET /orders/${i}`,
                startMs: Date.now() - i * 1000,
                durationUs: 100000,
                status: "ok",
                spanCount: 3,
              }));
            if (cmd === "get_insight_setup")
              return {
                paths: [],
                suggestions: [],
                stats: { detail: "Ready", spansIngested: 6000 },
                engines: [],
              };
            if (
              cmd === "get_server_diagnostics" ||
              cmd === "get_infra_snapshot"
            )
              return [];
            if (cmd.startsWith("plugin:event|")) return ++callback;
            if (cmd.startsWith("list_")) return [];
            return null;
          },
        };
      },
      { theme },
    );
    const page = await context.newPage();
    page.on("pageerror", (e) => errors.push(`${theme}: ${e.message}`));
    page.on("response", response => { if (response.status() >= 400) errors.push(`${theme}: HTTP ${response.status()} ${response.url()}`); });
    await page.goto(base);
    await page
      .getByRole("heading", { name: "Your servers, in one clear view" })
      .waitFor().catch(async error => { await page.screenshot({ path: path.join(output, `${theme}-failure.png`), fullPage: true }); console.error(await page.locator("body").innerText()); throw error; });
    assert.ok(page.url().endsWith("/welcome"));
    await audit(page, `${theme}-welcome`);
    await page.getByRole("button", { name: "Get started" }).click();
    await page
      .getByRole("button", { name: "Import servers", exact: true })
      .click();
    const dialog = page.getByRole("dialog");
    await dialog.getByRole("combobox").click();
    await page.getByRole("option", { name: "CSV", exact: true }).click();
    await page
      .getByLabel("Or paste CSV rows")
      .fill(
        "name,host,username,labels\nProduction,prod.example.com,deploy,production|web\nStaging,staging.example.com,deploy,staging\nAlias,PROD.EXAMPLE.COM.,deploy,web\n",
      );
    await dialog.getByRole("button", { name: "Preview import" }).click();
    await dialog.getByText("Duplicate of Production").waitFor();
    assert.equal(
      await dialog.getByRole("checkbox", { name: "Import Alias" }).isDisabled(),
      true,
    );
    await audit(page, `${theme}-import`);
    await dialog
      .getByRole("button", { name: "Import 2 servers", exact: true })
      .click();
    await dialog.getByText("2 imported", { exact: true }).waitFor();
    await dialog
      .getByRole("button", { name: "Close", exact: true })
      .first()
      .click();
    await page.getByRole("button", { name: "Continue", exact: true }).click();
    await page.evaluate(() => {
      window.__trackB.failConnection = true;
    });
    await page.getByRole("button", { name: "Run connection test" }).click();
    await page.getByText("Failed", { exact: true }).waitFor();
    await page
      .getByText("Connection timed out. Check your VPN", { exact: false })
      .waitFor();
    await page.evaluate(() => {
      window.__trackB.failConnection = false;
    });
    await page.getByRole("button", { name: "Run connection test" }).click();
    await page
      .getByRole("button", { name: "Run again", exact: true })
      .waitFor();
    assert.equal(await page.getByText("Passed", { exact: true }).count(), 4);
    await audit(page, `${theme}-connect`);
    await page.getByRole("button", { name: "Continue", exact: true }).click();
    await page.getByRole("button", { name: "Run Setup Guide checks" }).click();
    await page
      .getByText("1 passed · 1 warnings · 0 failed.", { exact: false })
      .waitFor();
    await audit(page, `${theme}-setup`);
    await page.getByRole("button", { name: "Continue", exact: true }).click();
    await page.getByRole("button", { name: "Discover applications" }).click();
    await page
      .getByRole("checkbox", { name: "Worker", exact: false })
      .uncheck();
    await audit(page, `${theme}-monitor`);
    await page.getByRole("button", { name: "Save selection" }).click();
    assert.equal(
      await page.evaluate(
        () =>
          window.__trackB.calls.find(
            (c) => c.cmd === "save_onboarding_app_selection",
          ).args.selectedIds.length,
      ),
      1,
    );
    await page.getByRole("button", { name: "Go to dashboard" }).click();
    await page
      .getByRole("heading", { name: "All servers", exact: true })
      .last()
      .waitFor();
    await audit(page, `${theme}-fleet-grid`);
    await page.getByRole("button", { name: "Table view", exact: true }).click();
    await page
      .getByRole("columnheader", { name: "CPU", exact: false })
      .waitFor();
    await audit(page, `${theme}-fleet-table`);
    await page.getByLabel("Search servers or tags").fill("production");
    await page.getByText("1 of 2 servers", { exact: false }).waitFor();
    await jump(page, "Settings");
    await page
      .getByRole("heading", { name: "Settings", exact: true })
      .waitFor();
    await page.getByRole("link", { name: "Open setup wizard" }).waitFor();
    await jump(page, "All servers");
    assert.equal(
      await page.getByLabel("Search servers or tags").inputValue(),
      "production",
    );
    await page
      .getByRole("button", { name: "Clear filters", exact: true })
      .click();
    await page.getByRole("button", { name: "Export", exact: true }).click();
    await page.getByText("Server profiles exported without secrets").waitFor();
    assert.ok(
      await page.evaluate(() =>
        window.__trackB.calls.some((c) => c.cmd === "export_server_profiles"),
      ),
    );
    await page.keyboard.press("?");
    await page
      .getByRole("dialog")
      .getByText("Keyboard shortcuts", { exact: true })
      .waitFor();
    await audit(page, `${theme}-shortcuts`);
    await page.keyboard.press("Escape");
    await page.keyboard.press("Control+k");
    const commandInput = page.getByRole("combobox", {
      name: "Search commands",
    });
    await commandInput.fill("Orders API");
    await page.getByRole("option").first().waitFor();
    await audit(page, `${theme}-palette`);
    await commandInput.press("ArrowDown");
    await commandInput.press("Enter");
    assert.ok(page.url().includes("/apps/app-1"));
    await jump(page, "Traces");
    await page.getByText("2000 traces", { exact: true }).waitFor();
    assert.ok((await page.locator("tbody tr[data-index]").count()) < 100);
    const traceRegion = page.getByRole("region", { name: "Trace results" });
    await traceRegion.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    await page
      .getByRole("link", { name: "GET /orders/1999", exact: true })
      .waitFor();
    await audit(page, `${theme}-traces`);
    await page.getByLabel("Service", { exact: true }).fill("remember-orders");
    await jump(page, "All servers");
    await jump(page, "Traces");
    assert.equal(
      await page.getByLabel("Service", { exact: true }).inputValue(),
      "remember-orders",
    );
    await jump(page, "All servers");
    await page.setViewportSize({ width: 800, height: 900 });
    await page.getByRole("button", { name: "Grid view", exact: true }).click();
    await audit(page, `${theme}-fleet-compact`);
    assert.equal(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
      true,
    );
    await page.setViewportSize({ width: 1360, height: 1000 });
    await jump(page, "Web traffic");
    await page
      .getByRole("textbox", { name: "Search raw log line" })
      .fill("remember-web");
    await jump(page, "All servers");
    await jump(page, "Web traffic");
    assert.equal(
      await page
        .getByRole("textbox", { name: "Search raw log line" })
        .inputValue(),
      "remember-web",
    );
    await jump(page, "Logs");
    await page.getByLabel("Search", { exact: true }).fill("remember-log");
    await jump(page, "All servers");
    await jump(page, "Logs");
    assert.equal(
      await page.getByLabel("Search", { exact: true }).inputValue(),
      "remember-log",
    );
    await page.getByLabel("Search", { exact: true }).focus();
    await page.keyboard.press("?");
    assert.equal(await page.getByRole("dialog").count(), 0);
    await page.getByLabel("Search", { exact: true }).fill("remember-log");
    await jump(page, "Settings");
    await page.getByRole("link", { name: "Open setup wizard" }).click();
    await page
      .getByRole("heading", { name: "Your servers, in one clear view" })
      .waitFor();
    await page.getByRole("button", { name: "Skip for now" }).click();
    await page
      .getByRole("heading", { name: "All servers", exact: true })
      .last()
      .waitFor();
    await context.close();
  }
} finally {
  await browser.close();
  await fs.writeFile(
    path.join(output, "report.json"),
    JSON.stringify({ errors, audits }, null, 2),
  );
}
assert.deepEqual(errors, [], "Browser runtime errors");
assert.deepEqual(
  audits.filter((a) => a.violations.length),
  [],
  "Accessibility violations",
);
console.log(
  `Track B browser checks passed with ${audits.length} WCAG AA audits`,
);
