export type PanelKind = "chart" | "stat" | "logs" | "redis"
export interface Panel { id: string, kind: PanelKind, title: string, metric: string, x: number, y: number, w: number, h: number }
export interface Dashboard { version: 1, id: string, name: string, server: string, panels: Panel[] }
export function parseDashboard(raw: string): Dashboard {
  if (raw.length > 131072) throw new Error("Dashboard exceeds 128 KiB")
  const d = JSON.parse(raw)
  if (d.version !== 1 || typeof d.id !== "string" || !d.id || typeof d.name !== "string" || !d.name || typeof d.server !== "string" || !Array.isArray(d.panels) || d.panels.length > 32) throw new Error("Invalid dashboard document")
  const ids = new Set<string>()
  for (const p of d.panels) {
    if (typeof p.id !== "string" || ids.has(p.id) || !["chart", "stat", "logs", "redis"].includes(p.kind) || typeof p.title !== "string" || typeof p.metric !== "string" || ![p.x,p.y,p.w,p.h].every(Number.isInteger) || p.x < 0 || p.y < 0 || p.w < 1 || p.h < 2 || p.x + p.w > 12 || p.h > 40 || p.y > 1000) throw new Error("Invalid panel layout")
    ids.add(p.id)
  }
  return d
}
const panel = (id: string, title: string, metric: string, x: number, y: number, kind: PanelKind = "chart"): Panel => ({ id, title, metric, kind, x, y, w: 6, h: kind === "stat" ? 3 : 6 })
export const STARTERS: Dashboard[] = [
  { version: 1, id: "web-app", name: "Web app", server: "$server", panels: [panel("requests","Requests per second","web.req_per_sec",0,0),panel("latency","Endpoint p95 ms","endpoint.p95_ms",6,0),panel("errors","HTTP 5xx percent","http.5xx_rate_percent",0,6,"stat"),panel("logs","Application logs","",6,6,"logs")] },
  { version: 1, id: "node-pm2", name: "Node + PM2", server: "$server", panels: [panel("cpu","CPU percent","cpu.percent",0,0),panel("memory","Memory percent","memory.percent",6,0),panel("logs","PM2 logs","",0,6,"logs"),panel("redis","Redis stats","",6,6,"redis")] },
  { version: 1, id: "docker-host", name: "Docker host", server: "$server", panels: [panel("cpu","CPU percent","cpu.percent",0,0),panel("memory","Memory percent","memory.percent",6,0),panel("disk","Disk percent","disk.percent",0,6),panel("load","Load average","load.1",6,6)] },
]
export interface Baseline { median: number | null, mad: number | null, samples: number }
export function isAnomaly(value: number, b: Baseline): boolean | null {
  if (b.samples < 12 || b.median === null || b.mad === null || !Number.isFinite(value)) return null
  return Math.abs(value - b.median) > Math.max(3 * 1.4826 * b.mad, Math.abs(b.median) * .05, 1e-6)
}
export function anomalyRate(values: number[], b: Baseline): number | null {
  const states = values.map(v => isAnomaly(v,b)).filter(v => v !== null)
  return states.length ? states.filter(Boolean).length / states.length : null
}
