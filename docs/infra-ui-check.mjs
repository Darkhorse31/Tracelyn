// Run: node docs/infra-ui-check.mjs <verification node_modules> [base URL]
// Dependencies: playwright and axe-core. The test mocks Tauri IPC; it never contacts a VPS.
import { createRequire } from 'node:module';
import fs from 'node:fs/promises';
import path from 'node:path';
const require = createRequire(path.resolve(process.argv[2], 'package.json'));
const { chromium } = require('playwright');
const axePath = require.resolve('axe-core/axe.min.js');
const base = process.argv[3] || 'http://localhost:1434';
const output = path.resolve(process.argv[2], '..', 'results');
await fs.mkdir(output, { recursive: true });
const browser = await chromium.launch({ executablePath: 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', headless: true });
const sections = ['databases', 'queues', 'balancers', 'containers', 'certificates', 'cron', 'capacity', 'security'];
const report = [];
let errors = [];
try {
  for (const theme of ['light', 'dark']) {
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, reducedMotion: 'reduce' });
    await context.addInitScript(({ theme }) => {
      localStorage.setItem('tracelyn-theme', theme);
      let callback = 0;
      const now = Date.now();
      const resource = (name, state, fields = {}) => ({ id: name, name, state, fields });
      const snapshots = [];
      const snapshot = (collector, rows, phase = 'ok', message = `${rows.length} resources`) => snapshots.push({ collector, rows, phase, message, fixHint: phase === 'ok' ? null : 'Check monitoring permissions and credentials for this service.', sampledAtMs: now });
      for (const engine of ['mysql', 'postgres', 'mongo']) snapshot(engine, [resource(engine, 'ok', { connections: 12, qps: 120.4, slowQueries: 2, cacheHitPercent: 98.4, replicationLagSeconds: 0.3, sizes: [{ database: 'orders', bytes: 83886080 }], slowQueryScope: 'Read-only source metrics; throughput uses two samples.' })]);
      snapshot('rabbitmq', [resource('/ / jobs', 'running', { depth: 42, ready: 40, unacknowledged: 2, consumers: 4, publishPerSecond: 3.4, deliverPerSecond: 3.1 })]);
      snapshot('kafka', [resource('workers / orders / 0', 'warning', { partitions: 3, currentOffset: 80, logEndOffset: 100, lag: 20, consumers: 1, consumePerSecond: 10, producePerSecond: 12, consumer: 'consumer-abc' })]);
      for (const kind of ['haproxy', 'nginx_upstreams', 'traefik', 'caddy']) snapshot(kind, [resource('orders / 127.0.0.1:3000', 'up', { weight: 3, errorPercent: 0.4, tcp: 'up', rate: 120, recentConnectionErrors: 1, note: 'TCP reachability is a point-in-time probe. Log error rates use attributable sampled requests.' }), resource('orders / 127.0.0.1:3001', 'down', { weight: 1, tcp: 'down' })]);
      snapshot('docker_infra', Array.from({ length: 31 }, (_, i) => resource(`orders-${i}`, 'running', { cpuPercent: 12.5, memory: '42MiB / 1GiB', memoryPercent: 4.1, restarts: 3, health: 'healthy', ports: '0.0.0.0:8080 → 3000/tcp', composeProject: 'shop' })));
      snapshot('kubernetes', [resource('Node / cluster / vps', 'ready', { kind: 'Node' }), resource('Pod / default / orders', 'Pending', { kind: 'Pod', node: 'vps', restarts: 3, readyContainers: 0, reason: 'Unschedulable' })]);
      snapshot('certificates', [resource('/etc/letsencrypt/live/example.com/cert.pem', 'warning', { daysRemaining: 14, expiresAtMs: now + 14 * 86400000, subject: 'CN=example.com' })]);
      snapshot('certificates_pc', [resource('example.com', 'critical', { daysRemaining: 4, expiresAtMs: now + 4 * 86400000, subject: 'CN=example.com', verified: true })]);
      snapshot('cron', [resource('deploy · /usr/local/bin/backup', 'scheduled', { user: 'deploy', source: 'user crontab', schedule: '0 2 * * *', timezone: 'UTC', nextRunMs: now + 3600000, lastResult: 'Dispatch was recorded; no exit result logged' })]);
      snapshot('capacity', [resource('/', 'warning', { usedPercent: 80, usedBytes: 80e9, totalBytes: 100e9, growthBytesPerDay: 2e9, daysUntilFull: 10, rSquared: 0.92, sampleCount: 48, confidence: 'Higher confidence · R² 0.92 · 14 days observed. Linear trend assumes unchanged workload.', history: Array.from({ length: 48 }, (_, i) => ({ t: now - (48 - i) * 3600000, v: 60 + i * 0.4 })) })]);
      snapshot('ssh_security', [resource('198.51.100.4', 'warning', { attempts: 12, timeline: [{ hour: '2026-10-26T01', count: 5 }, { hour: '2026-10-26T02', count: 7 }] })]);
      snapshot('ports', [resource('tcp 0.0.0.0:22', 'exposed', { address: '0.0.0.0:22', process: 'users:(("sshd",pid=42,fd=3))' })]);
      snapshot('updates', [resource('openssl/noble-security', 'security', { version: '3.0.13', source: 'noble-security' })]);
      snapshot('fail2ban', [resource('sshd', 'ok', { banned: 2, failed: 42, bannedIps: '198.51.100.4 203.0.113.9' })]);
      snapshot('last_logins', [resource('deploy', 'recorded', { session: 'deploy pts/0 198.51.100.5 Mon Oct 26 01:00:00 2026 still logged in' })]);
      const checks = [{ check: { id: 'health', serverId: 'fixture', url: 'https://example.com/health', intervalSeconds: 60, enabled: true }, history: Array.from({ length: 12 }, (_, i) => ({ tsMs: now - (12 - i) * 60000, ok: true, statusCode: 200, addresses: ['93.184.216.34:443'], dnsMs: 2, connectMs: 10, tlsMs: 15, ttfbMs: 20, totalMs: 47 + i, error: null })) }];
      window.__infraMode = 'populated';
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
        transformCallback: () => ++callback, unregisterCallback() {},
        invoke: async (cmd, args) => {
          if (cmd === 'list_servers') return [{ id: 'fixture', name: 'Production VPS', host: '192.0.2.1', port: 22, username: 'deploy', authType: 'key', labels: [] }];
          if (cmd === 'list_server_statuses') return [{ id: 'fixture', status: 'connected', hostname: 'vps.example.com', osInfo: 'Linux', lastError: null }];
          if (cmd === 'get_infra_snapshot') { if (window.__infraMode === 'error') throw Error('Fixture IPC failure'); return window.__infraMode === 'empty' ? snapshots.map(s => ({ ...s, rows: [], phase: 'not_installed', fixHint: 'Make the read-only CLI available and verify permissions.' })) : snapshots; }
          if (cmd === 'get_infra_target') return { target: { host: '', port: null, container: '', database: '' }, hasCredentials: false };
          if (cmd === 'list_infra_domains') return ['example.com', 'api.example.com'];
          if (cmd === 'list_uptime_checks') return checks;
          if (cmd === 'upsert_uptime_check') { const check = { ...args.check, id: args.check.id || `check-${checks.length}` }; const index = checks.findIndex(c => c.check.id === check.id); if (index >= 0) checks[index].check = check; else checks.push({ check, history: [] }); return check; }
          if (cmd === 'delete_uptime_check') { const index = checks.findIndex(c => c.check.id === args.checkId); if (index >= 0) checks.splice(index, 1); return null; }
          if (cmd === 'run_uptime_check') return checks[0].history[0];
          if (cmd === 'get_server_diagnostics') return snapshots.map(s => ({ collector: `infra:${s.collector}`, status: 'ok', message: s.message, fixHint: null }));
          if (cmd.startsWith('plugin:event|')) return ++callback;
          if (cmd.startsWith('list_')) return [];
          return null;
        },
      };
    }, { theme });
    const page = await context.newPage();
    page.on('pageerror', e => errors.push(`${theme}: ${e.message}`));
    for (const width of [1280, 1024]) {
      await page.setViewportSize({ width, height: 900 });
      for (const section of sections) {
        await page.goto(`${base}/infra/${section}`);
        await page.locator('h2').first().waitFor();
        await page.getByText('Collectors reporting', { exact: true }).waitFor();
        await page.locator('table').first().waitFor();
        // Wait for web fonts and the rendered theme before axe measures text contrast.
        await page.evaluate(() => document.fonts.ready);
        await page.waitForFunction(() => {
          const heading = document.querySelector('h2');
          if (!heading) return false;
          const style = getComputedStyle(heading);
          const animations = document.getAnimations().filter(animation =>
            animation.effect?.getComputedTiming().endTime !== Infinity && animation.playState === 'running');
          return style.color === getComputedStyle(document.body).color &&
            style.webkitTextFillColor === style.color && animations.length === 0;
        });
        await page.addScriptTag({ path: axePath });
        const result = await page.evaluate(async () => {
          const audit = await window.axe.run(document.querySelector('main') || document, { runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa'] } });
          return { violations: audit.violations.map(v => ({ id: v.id, impact: v.impact, nodes: v.nodes.map(n => ({ target: n.target, summary: n.failureSummary })) })), overflow: document.documentElement.scrollWidth > innerWidth };
        });
        report.push({ theme, width, section, ...result });
        if (width === 1280 && ['databases', 'certificates', 'capacity', 'security'].includes(section)) await page.screenshot({ path: path.join(output, `${theme}-${section}.png`), fullPage: true });
      }
    }
    // Exercise actual controls through mocked commands and verify pagination.
    await page.goto(`${base}/infra/containers`);
    await page.getByRole('button', { name: 'Next', exact: true }).first().click();
    await page.getByText('Page 2 of 2', { exact: false }).waitFor();
    await page.goto(`${base}/infra/certificates`);
    await page.getByLabel('Check URL', { exact: true }).fill('https://new.example.com/health');
    await page.getByRole('button', { name: 'Add check', exact: true }).click();
    await page.getByRole('button', { name: 'https://new.example.com/health', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Pause https://new.example.com/health', exact: true }).click();
    await page.getByRole('button', { name: 'Resume https://new.example.com/health', exact: true }).waitFor();
    await page.getByRole('button', { name: 'Remove https://new.example.com/health', exact: true }).click();
    await page.getByRole('button', { name: 'https://new.example.com/health', exact: true }).waitFor({ state: 'detached' });
    await page.goto(`${base}/infra/databases`);
    await page.evaluate(() => { window.__infraMode = 'empty'; });
    await page.getByRole('button', { name: 'Refresh collectors' }).click();
    await page.getByText('Not installed', { exact: true }).first().waitFor();
    await page.evaluate(() => { window.__infraMode = 'error'; });
    await page.getByRole('button', { name: 'Refresh collectors' }).click();
    await page.getByText('Could not load infrastructure', { exact: true }).waitFor();
    await context.close();
  }
} finally { await browser.close(); }
await fs.writeFile(path.join(output, 'ui-audit.json'), JSON.stringify({ report, errors }, null, 2));
const violations = report.reduce((n, r) => n + r.violations.length, 0);
const overflow = report.filter(r => r.overflow).length;
console.log(JSON.stringify({ screens: report.length, violations, overflow, errors, artifacts: output }, null, 2));
if (violations || overflow || errors.length) process.exitCode = 1;
