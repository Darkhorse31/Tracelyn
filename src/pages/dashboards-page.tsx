import { useEffect, useRef, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import GridLayout from "react-grid-layout/legacy"
import "react-grid-layout/css/styles.css"
import "react-resizable/css/styles.css"
import { useServersContext } from "@/contexts/servers-context"
import { usePrivacy } from "@/contexts/privacy-context"
import { useLicense } from "@/contexts/license-context"
import { useTimeRange } from "@/contexts/time-range-context"
import { MetricUplot } from "@/components/charts/metric-uplot"
import { AnomalyRibbon, WhatChanged } from "@/components/charts/baseline-insights"
import { MetricCard } from "@/components/web/metric-card"
import { LogExplorer } from "@/components/logs/log-explorer"
import { RedisStatCards } from "@/components/redis/redis-stat-cards"
import { useRedisDashboard } from "@/hooks/use-redis-dashboard"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { queryMetricSeries } from "@/lib/metrics-api"
import { parseDashboard, STARTERS, type Dashboard, type Panel } from "@/lib/dashboards"
import type { MetricPoint } from "@/lib/types"

function RedisPanel({serverId}:{serverId:string}) {
  const { snapshot, error, loading } = useRedisDashboard(serverId)
  return error ? <p role="status">Redis data unavailable</p> : loading ? <p>Loading Redis</p> : <RedisStatCards derived={snapshot.derived} />
}
function MetricPanel({ panel, serverId }: { panel: Panel, serverId: string }) {
  const { range, isLive } = useTimeRange()
  const { enabled } = usePrivacy()
  const [points, setPoints] = useState<MetricPoint[]>([])
  const [error, setError] = useState(false)
  useEffect(() => {
    let active = true
    setPoints([])
    const load = async () => {
      try {
        const result = await queryMetricSeries({ serverId, metricKey: panel.metric, rangeStartMs: range.startMs, rangeEndMs: range.endMs, preferLive: isLive })
        if (active) { setPoints(result.points.map(p => ({ t:p.tsMs, v:p.value }))); setError(false) }
      } catch { if (active) setError(true) }
    }
    void load()
    const timer = isLive ? window.setInterval(() => void load(), 10000) : undefined
    return () => { active = false; window.clearInterval(timer) }
  }, [serverId, panel.metric, range.startMs, range.endMs, isLive])
  const title = enabled ? "Metric panel" : panel.title
  if (error) return <p role="status">Metric data unavailable</p>
  return panel.kind === "stat" ? <MetricCard title={title} value={points[points.length-1]?.v.toFixed(1) ?? "No samples"} /> : <><MetricUplot title={title} points={points} height={128} /><AnomalyRibbon serverId={serverId} metric={panel.metric} points={points} /></>
}
export function DashboardsPage() {
  const { servers, selectedServerId } = useServersContext()
  const { enabled, mask } = usePrivacy()
  const { license } = useLicense()
  const writable = license?.canWrite === true && !enabled
  const { range } = useTimeRange()
  const [saved, setSaved] = useState<Dashboard[]>([])
  const [dashboard, setDashboard] = useState<Dashboard>(() => structuredClone(STARTERS[0]))
  const [message, setMessage] = useState("")
  const [panelKind, setPanelKind] = useState<Panel["kind"]>("chart")
  const [metric, setMetric] = useState("cpu.percent")
  const container = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(1000)
  useEffect(() => {
    const observer = new ResizeObserver(entries => setWidth(entries[0].contentRect.width))
    if (container.current) observer.observe(container.current)
    void invoke<Dashboard[]>("documents", { kind: "dashboard" }).then(ds => setSaved(ds.map(d => parseDashboard(JSON.stringify(d))))).catch(() => setMessage("Could not load saved dashboards"))
    return () => observer.disconnect()
  }, [])
  const serverId = dashboard.server === "$server" ? selectedServerId : dashboard.server
  const update = (patch: Partial<Dashboard>) => { if (writable) setDashboard(d => ({ ...d, ...patch })) }
  const persist = async () => {
    if (!writable) return
    try {
      const d = parseDashboard(JSON.stringify(dashboard))
      await invoke("save_document", { kind:"dashboard", document:d })
      setSaved(ds => [...ds.filter(s => s.id !== d.id), d])
      setMessage("Dashboard saved locally")
    } catch { setMessage("Could not save dashboard") }
  }
  return <div className="space-y-6 p-6" ref={container}>
    <div className="space-y-2"><h1 className="text-2xl font-semibold">Dashboards</h1><p className="text-sm text-muted-foreground">Compose panels and bind the server variable. Drag panel handles or use the layout controls.</p></div>
    <div className="flex flex-wrap items-center gap-3">
      <select aria-label="Choose dashboard" className="rounded border border-border bg-background p-2" value="" onChange={e => { const d=[...saved,...STARTERS].find(d => d.id===e.target.value); if(d) setDashboard(structuredClone(d)) }}><option value="">Open dashboard</option>{saved.map(d => <option key={`saved-${d.id}`} value={d.id}>{enabled ? "Saved dashboard" : d.name}</option>)}{STARTERS.map(d => <option key={d.id} value={d.id}>{d.name} starter</option>)}</select>
      <Input aria-label="Dashboard name" className="w-60" disabled={!writable} value={enabled ? "Dashboard" : dashboard.name} onChange={e => update({ name:e.target.value })} />
      <select aria-label="Server variable" className="rounded border border-border bg-background p-2" value={dashboard.server} onChange={e => update({ server:e.target.value })} disabled={!writable}><option value="$server">Selected server variable</option>{servers.map(s => <option key={s.id} value={s.id}>{enabled ? "Server" : mask(s.name)}</option>)}</select>
      <Button disabled={!writable} onClick={() => void persist()}>Save</Button>
      <Button variant="outline" disabled={!writable} onClick={() => update({id:crypto.randomUUID(),name:`${dashboard.name} copy`})}>Duplicate</Button>
      <Button variant="outline" disabled={enabled} onClick={() => {
        const url=URL.createObjectURL(new Blob([JSON.stringify(dashboard,null,2)],{type:"application/json"}))
        const a=document.createElement("a"); a.href=url; a.download="dashboard.json"; a.click(); setTimeout(() => URL.revokeObjectURL(url),1000)
      }}>Export JSON</Button>
      <label className={`rounded border border-border p-2 text-sm ${!writable ? "opacity-50" : "cursor-pointer"}`}>Import JSON<input className="sr-only" type="file" accept=".json,application/json" disabled={!writable} onChange={async e => { const file=e.target.files?.[0]; if(!file)return; try { if(file.size>131072)throw Error(); const d=parseDashboard(await file.text()); setDashboard({...d,id:crypto.randomUUID()}); setMessage("Imported draft. Save to keep it") } catch { setMessage("Invalid dashboard JSON") } e.target.value="" }} /></label>
      <Button variant="ghost" disabled={!writable || !saved.some(d=>d.id===dashboard.id)} onClick={async () => {try { await invoke("delete_document",{kind:"dashboard",id:dashboard.id}); setSaved(ds=>ds.filter(d=>d.id!==dashboard.id)); setDashboard(structuredClone(STARTERS[0])); setMessage("Dashboard removed") }catch{setMessage("Could not remove dashboard")} }}>Delete</Button>
    </div>
    {!license?.canWrite && <p className="text-sm">Read-only license · editing and import are disabled</p>}
    <div className="flex flex-wrap items-center gap-3">
      <select aria-label="Panel type" className="rounded border border-border bg-background p-2" value={panelKind} disabled={!writable} onChange={e=>setPanelKind(e.target.value as Panel["kind"])}>{["chart","stat","logs","redis"].map(k=><option key={k}>{k}</option>)}</select>
      <Input aria-label="Metric key" value={enabled ? "" : metric} disabled={!writable} className="w-60" onChange={e=>setMetric(e.target.value)} />
      <Button disabled={!writable || dashboard.panels.length>=32} variant="outline" onClick={()=>update({panels:[...dashboard.panels,{id:crypto.randomUUID(),kind:panelKind,title:metric || panelKind,metric,x:0,y:Math.max(0,...dashboard.panels.map(p=>p.y+p.h)),w:6,h:6}]})}>Add panel</Button>
    </div>
    {message && <p role="status">{message}</p>}
    {serverId ? <>
      <WhatChanged serverId={serverId} start={Math.max(range.startMs,range.endMs-86400000)} end={range.endMs} />
      <GridLayout width={Math.max(280,width-48)} cols={12} rowHeight={32} margin={[16,16]} containerPadding={[0,0]} isDraggable={writable} isResizable={writable} draggableHandle=".panel-handle" layout={dashboard.panels.map(p=>({...p,i:p.id,minW:3,minH:3}))} onLayoutChange={layout=>{if(writable)setDashboard(d=>({...d,panels:d.panels.map(p=>{const l=layout.find(l=>l.i===p.id);return l?{...p,x:l.x,y:l.y,w:l.w,h:l.h}:p})}))}}>
        {dashboard.panels.map(p=><section key={p.id} className="overflow-auto rounded-lg border border-border bg-card text-card-foreground">
          <div className="flex items-center justify-between gap-2 border-b border-border p-3"><span className="panel-handle cursor-move text-sm font-medium">{enabled ? "Panel" : mask(p.title)}</span><div className="flex gap-2"><button className="min-h-8 rounded bg-muted px-2 text-xs text-foreground disabled:opacity-50" disabled={!writable} aria-label="Move panel left" onClick={()=>update({panels:dashboard.panels.map(v=>v.id===p.id?{...v,x:Math.max(0,v.x-1)}:v)})}>Left</button><button className="min-h-8 rounded bg-muted px-2 text-xs text-foreground disabled:opacity-50" disabled={!writable} aria-label="Move panel down" onClick={()=>update({panels:dashboard.panels.map(v=>v.id===p.id?{...v,y:v.y+1}:v)})}>Down</button><button className="min-h-8 rounded bg-muted px-2 text-xs text-foreground disabled:opacity-50" disabled={!writable} aria-label="Remove panel" onClick={()=>update({panels:dashboard.panels.filter(v=>v.id!==p.id)})}>Remove</button></div></div>
          <div className="p-3">{p.kind==="logs" ? <fieldset disabled={!writable} className="min-w-0"><LogExplorer serverId={serverId} title="Dashboard logs" /></fieldset> : p.kind==="redis" ? <RedisPanel serverId={serverId} /> : <MetricPanel panel={p} serverId={serverId} />}</div>
        </section>)}
      </GridLayout>
    </> : <p>Select a server to load panel data</p>}
  </div>
}
