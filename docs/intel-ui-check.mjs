// Run: node docs/intel-ui-check.mjs <verification node_modules> [production URL]
// All Tauri IPC is mocked; no notifications, credentials, or servers are touched.
import { createRequire } from 'node:module';
import fs from 'node:fs/promises';
import path from 'node:path';
const require = createRequire(path.resolve(process.argv[2], 'package.json'));
const { chromium } = require('playwright');
const axePath = require.resolve('axe-core/axe.min.js');
const base = process.argv[3] || 'http://127.0.0.1:1435';
const output = path.resolve(process.argv[2], '..', 'intel-merge-results');
await fs.mkdir(output, { recursive: true });
const browser = await chromium.launch({ executablePath: 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', headless: true });
const report = [], errors = [];
const tabs = ['incidents', 'rules', 'channels', 'anomalies', 'digest', 'sharing'];
try {
  for (const theme of ['light', 'dark']) {
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, reducedMotion: 'reduce' });
    await context.addInitScript(({ theme }) => {
      localStorage.setItem('tracelyn-theme', theme);
      let callback = 0;
      const now = Date.now();
      const incident = { id: 'incident', serverId: 'fixture', title: 'CPU threshold exceeded', severity: 'critical', status: 'open', openedAtMs: now - 60000, updatedAtMs: now };
      const timeline = [{ id: 'alert', tsMs: now - 60000, kind: 'alert', message: 'CPU exceeded 90 percent.' }];
      const config = { chatId: '', smtpHost: '', smtpPort: 587, smtpTls: 'starttls', mailFrom: '', mailTo: '', username: '' };
      const channel = { id: 'channel', name: 'Desktop', kind: 'windows', enabled: true, config, hasSecret: true, createdAtMs: now };
      const rule = { id: 'rule', name: 'CPU warning', serverId: null, kind: 'metric', enabled: true, severity: 'warning', params: { metricKey: 'cpu.percent', threshold: 90, durationSecs: 60, clearDurationSecs: 30, pattern: '', windowSecs: 60, host: '', port: 443 }, createdAtMs: now, updatedAtMs: now };
      const rows = {
        intel_list_incidents: [incident], intel_list_rules: [rule], intel_list_channels: [channel],
        intel_list_digests: [{ id: 'digest', name: 'Morning digest', cadence: 'daily', hourUtc: 8, weekday: 1, channelId: 'channel', enabled: true }],
        intel_list_views: [{ id: 'view', name: 'Service health', path: '/intel?tab=anomalies', note: 'Watch CPU trends.' }],
        intel_list_anomalies: [{ id: 'anomaly', serverId: 'fixture', metricKey: 'cpu.percent', tsMs: now, value: 92, baseline: 30, z: 4.2, message: 'CPU above its seasonal baseline.' }],
        intel_anomaly_series: Array.from({ length: 20 }, (_, i) => ({ tsMs: now - (20 - i) * 60000, value: i === 18 ? 92 : 30 + i % 3, baseline: 30, low: 20, high: 40, z: 0, anomalous: i === 18 })),
      };
      window.__intelMode = 'populated';
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
        transformCallback: () => ++callback, unregisterCallback() {},
        invoke: async (cmd, args) => {
          if (cmd === 'list_servers') return [{ id: 'fixture', name: 'Production VPS', host: '192.0.2.1', port: 22, username: 'deploy', authType: 'key', labels: [] }];
          if (cmd === 'list_server_statuses') return [{ id: 'fixture', status: 'connected', hostname: 'vps.example.com', osInfo: 'Linux', lastError: null }];
          if (cmd.startsWith('plugin:event|')) return ++callback;
          if (cmd.startsWith('intel_') && window.__intelMode === 'error') throw Error('Fixture IPC failure');
          if (cmd === 'intel_get_incident') return { incident, timeline };
          if (cmd === 'intel_ack_incident') { incident.status = 'ack'; return null; }
          if (cmd === 'intel_resolve_incident') { incident.status = 'resolved'; return null; }
          if (cmd === 'intel_add_incident_note') { timeline.push({ id: 'note', kind: 'note', tsMs: now, message: args.text }); return null; }
          if (rows[cmd]) return window.__intelMode === 'empty' ? [] : rows[cmd];
          if (cmd === 'ops_get_mode') return false;
          if (cmd.startsWith('list_') || cmd === 'ops_list_audit') return [];
          return null;
        },
      };
    }, { theme });
    const page = await context.newPage();
    page.on('pageerror', e => errors.push(`${theme}: ${e.message}`));
    async function audit(screen, width) {
      await page.evaluate(() => document.fonts.ready);
      await page.waitForFunction(() => !document.getAnimations().some(a => a.effect?.getComputedTiming().endTime !== Infinity && a.playState === 'running'));
      await page.addScriptTag({ path: axePath });
      const result = await page.evaluate(async () => {
        const result = await window.axe.run(document, { runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa'] } });
        return { violations: result.violations.map(v => ({ id: v.id, nodes: v.nodes.map(n => ({ target: n.target, summary: n.failureSummary })) })), overflow: document.documentElement.scrollWidth > innerWidth };
      });
      report.push({ theme, width, screen, ...result });
    }
    for (const width of [1280, 1024]) {
      await page.setViewportSize({ width, height: 900 });
      for (const tab of tabs) {
        await page.goto(`${base}/intel?tab=${tab}`);
        await page.getByRole('tab', { selected: true }).waitFor();
        await page.locator('[role="tabpanel"] [data-slot="skeleton"]').waitFor({ state: 'detached' });
        await page.locator('nav[aria-label="Infrastructure"]').waitFor();
        await page.locator('nav[aria-label="Operations"]').waitFor();
        await page.locator('nav[aria-label="Teams and intelligence"] a[aria-current="page"]').waitFor();
        await audit(tab, width);
        if (width === 1280) await page.screenshot({ path: path.join(output, `${theme}-${tab}.png`), fullPage: true });
      }
      await page.goto(`${base}/intel/incidents/incident`);
      await page.getByRole('heading', { name: 'CPU threshold exceeded' }).waitFor();
      await audit('incident-detail', width);
    }
    for (const [tab, button] of [['rules', 'New rule'], ['channels', 'New channel']]) {
      await page.goto(`${base}/intel?tab=${tab}`);
      await page.getByRole('button', { name: button, exact: true }).first().click();
      await page.getByRole('dialog').waitFor();
      await audit(`${tab}-dialog`, 1024);
      await page.keyboard.press('Escape');
      await page.getByRole('dialog').waitFor({ state: 'detached' });
    }
    await page.goto(`${base}/intel/incidents/incident`);
    await page.getByRole('button', { name: 'Acknowledge', exact: true }).click();
    await page.getByText('Acknowledged', { exact: true }).waitFor();
    await page.getByLabel('Note', { exact: true }).fill('Investigating the CPU spike.');
    await page.getByRole('button', { name: 'Add note', exact: true }).click();
    await page.getByText('Investigating the CPU spike.', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Resolve', exact: true }).click();
    await page.getByText('Resolved', { exact: true }).waitFor();
    await page.goto(`${base}/intel?tab=incidents`);
    await page.getByRole('tab', { name: 'Incidents', exact: true }).focus();
    await page.keyboard.press('ArrowRight');
    await page.getByRole('tab', { name: 'Rules', selected: true }).waitFor();
    await page.keyboard.press('End');
    await page.getByRole('tab', { name: 'Sharing', selected: true }).waitFor();
    await page.keyboard.press('Home');
    await page.getByRole('tab', { name: 'Incidents', selected: true }).waitFor();
    await page.keyboard.press('ArrowLeft');
    await page.getByRole('tab', { name: 'Sharing', selected: true }).waitFor();
    const keyboard = await page.getByRole('tablist').evaluate(list => ({
      stops: [...list.querySelectorAll('[role="tab"]')].filter(tab => tab.tabIndex === 0).length,
      focused: document.activeElement?.getAttribute('aria-selected'),
    }));
    if (keyboard.stops !== 1 || keyboard.focused !== 'true') errors.push(`${theme}: tab focus did not follow selection`);
    for (const mode of ['empty', 'error']) {
      for (const tab of tabs) {
        await page.goto(`${base}/intel?tab=${tab}`);
        await page.evaluate(mode => { window.__intelMode = mode; }, mode);
        // Switch away and back to reload the panel using the new fixture mode.
        await page.getByRole('tab', { name: tab === 'rules' ? 'Channels' : 'Rules', exact: true }).click();
        await page.getByRole('tab', { name: tab[0].toUpperCase() + tab.slice(1), exact: true }).click();
        if (mode === 'error') await page.getByText(/Could not load/).first().waitFor();
        else await page.locator('[role="tabpanel"] [data-slot="skeleton"]').waitFor({ state: 'detached' });
        await audit(`${tab}-${mode}`, 1024);
      }
    }
    await context.close();
  }
} finally { await browser.close(); }
await fs.writeFile(path.join(output, 'ui-audit.json'), JSON.stringify({ report, errors }, null, 2));
const violations = report.reduce((n, r) => n + r.violations.length, 0);
const overflow = report.filter(r => r.overflow).length;
console.log(JSON.stringify({ screens: report.length, violations, overflow, errors, artifacts: output }, null, 2));
if (violations || overflow || errors.length) process.exitCode = 1;
