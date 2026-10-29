import { useEffect, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { usePrivacy } from "@/contexts/privacy-context"
import { Button } from "@/components/ui/button"
import { anomalyRate, type Baseline } from "@/lib/dashboards"
import type { MetricPoint } from "@/lib/types"

export function AnomalyRibbon({ serverId, metric, points }: { serverId: string, metric: string, points: MetricPoint[] }) {
  const [baselines, setBaselines] = useState<Record<number, Baseline>>({})
  const hours = [...new Set(points.map(p => Math.floor(p.t / 3600000)))].slice(-168).join(",")
  useEffect(() => {
    let active = true
    setBaselines({})
    void Promise.all(hours.split(",").filter(Boolean).map(async hour => {
      const at = Number(hour) * 3600000
      const b = await invoke<Baseline>("baseline", { serverId, metric, at })
      return [Number(hour), b] as const
    })).then(entries => { if (active) setBaselines(Object.fromEntries(entries)) }).catch(() => {})
    return () => { active = false }
  }, [serverId, metric, hours])
  const groups = Object.entries(baselines).map(([hour,b]) => ({ hour, rate: anomalyRate(points.filter(p => Math.floor(p.t / 3600000) === Number(hour)).map(p => p.v), b) }))
  const known = groups.filter(g => g.rate !== null)
  return <div className="space-y-2 border-t border-border p-3 text-xs" aria-label="Anomaly rate by UTC hour of week">
    <p>{known.length ? `Anomaly rate ${Math.round(100 * known.reduce((n,g) => n + g.rate!,0) / known.length)}%` : "Baseline warming up · needs 12 earlier samples per UTC hour of week"}</p>
    <div className="flex gap-1">{groups.map(g => <span key={g.hour} title={`${new Date(Number(g.hour)*3600000).toLocaleString()} · ${g.rate === null ? "Warming up" : `${Math.round(g.rate*100)}% anomalous`}`} className={`h-2 flex-1 rounded ${g.rate === null ? "bg-muted" : g.rate > 0 ? "bg-status-warning" : "bg-status-ok"}`} />)}</div>
  </div>
}

interface Comparison {
  movers: {metric:string,before:number,after:number,percent:number|null}[]
  newLogPatterns:string[]
  slowerEndpoints:{endpoint:string,beforeMs:number,afterMs:number}[]
  infrastructure:{available:boolean,message?:string,changes?:{collector:string,change:string,resource:{name:string,state:string}}[]}
  coverage:string
}
export function WhatChanged({ serverId, start, end }: { serverId: string, start: number, end: number }) {
  const { enabled, mask } = usePrivacy()
  const [result, setResult] = useState<Comparison | null>(null)
  const [error, setError] = useState("")
  const [busy, setBusy] = useState(false)
  useEffect(() => { setResult(null); setError("") }, [serverId])
  const format = (v:number) => v.toLocaleString(undefined,{maximumFractionDigits:2})
  return <section className="space-y-3">
    <Button variant="outline" disabled={busy} onClick={async () => {
      setBusy(true)
      setError("")
      try { setResult(await invoke<Comparison>("what_changed", { serverId, start, end })) }
      catch { setError("Could not compare stored windows") }
      finally { setBusy(false) }
    }}>{busy ? "Comparing windows" : "What changed"}</Button>
    {error && <p role="status">{error}</p>}
    {result && <div className="space-y-4 rounded border border-border bg-card p-4 text-card-foreground">
      <p className="text-sm font-medium">Selected window compared with the same window yesterday</p>
      {enabled ? <p>Comparison details hidden in privacy mode</p> : <>
        <div className="overflow-auto"><table className="w-full text-left text-sm"><caption className="mb-2 text-left font-medium">Top metric movers</caption><thead><tr className="border-b border-border">{["Metric","Yesterday","Selected window","Change"].map(h=><th className="p-2" key={h}>{h}</th>)}</tr></thead><tbody>{result.movers.map(m=><tr key={m.metric} className="border-b border-border"><th className="p-2 font-normal">{mask(m.metric)}</th><td className="p-2">{format(m.before)}</td><td className="p-2">{format(m.after)}</td><td className="p-2">{m.percent===null?"New from zero":format(m.percent)+"%"}</td></tr>)}</tbody></table>{!result.movers.length&&<p className="p-2 text-sm">No comparable metric samples in both windows</p>}</div>
        <div className="grid gap-4 lg:grid-cols-2"><div className="space-y-2"><h3 className="text-sm font-medium">New log patterns</h3>{result.newLogPatterns.length?result.newLogPatterns.slice(0,50).map((p,i)=><p className="break-all rounded border border-border p-2 font-mono text-xs" key={i}>{mask(p)}</p>):<p className="text-sm">No new patterns in stored logs</p>}</div><div className="space-y-2"><h3 className="text-sm font-medium">Slower endpoints</h3>{result.slowerEndpoints.length?result.slowerEndpoints.map((e,i)=><p key={i} className="rounded border border-border p-2 text-sm">{mask(e.endpoint)} ? {format(e.beforeMs)} ? {format(e.afterMs)} ms</p>):<p className="text-sm">No comparable endpoint slowdown</p>}</div></div>
        <div className="space-y-2"><h3 className="text-sm font-medium">Listening ports, packages and security</h3>{!result.infrastructure.available?<p className="text-sm">{result.infrastructure.message}</p>:result.infrastructure.changes?.length?result.infrastructure.changes.slice(0,100).map((c,i)=><p key={i} className="rounded border border-border p-2 text-sm">{mask(c.collector)} ? {c.change} ? {mask(c.resource.name)} ? {mask(c.resource.state)}</p>):<p className="text-sm">No infrastructure changes found</p>}</div>
        <p className="text-xs text-muted-foreground">{result.coverage}</p>
      </>}
      <Button variant="ghost" onClick={() => setResult(null)}>Close comparison</Button>
    </div>}
  </section>
}
