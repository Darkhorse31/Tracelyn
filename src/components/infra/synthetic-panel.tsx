import { useEffect, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { usePrivacy } from "@/contexts/privacy-context"
import { useLicense } from "@/contexts/license-context"
import { Button } from "@/components/ui/button"
import { MetricUplot } from "@/components/charts/metric-uplot"
interface Result { tsMs:number, ok:boolean, vantage:string, steps:{ok:boolean,status:number,totalMs:number,dnsMs:number,addresses:string,certificateDays:number|null,message:string}[] }
const example = { id:"", serverId:"", vantage:"pc", intervalSeconds:60, enabled:true, steps:[{url:"https://example.com/health",kind:"http",status:200,keyword:"",extract:{}}] }
export function SyntheticPanel({serverId}:{serverId:string}) {
  const {enabled}=usePrivacy()
  const {license}=useLicense()
  const writable=license?.canWrite===true && !enabled
  const [draft,setDraft]=useState(JSON.stringify({...example,id:crypto.randomUUID(),serverId},null,2))
  const [checks,setChecks]=useState<(typeof example)[]>([])
  const [history,setHistory]=useState<Result[]>([])
  const [selected,setSelected]=useState("")
  const [message,setMessage]=useState("")
  const [busy,setBusy]=useState(false)
  useEffect(()=>{
    let active=true
    const load=()=>void invoke<(typeof example)[]>("documents",{kind:"synthetic"}).then(cs=>{if(active)setChecks(cs.filter(c=>c.serverId===serverId))}).catch(()=>{if(active)setMessage("Could not load synthetic workflows")})
    load()
    const timer=setInterval(load,15000)
    setHistory([])
    setSelected("")
    setDraft(JSON.stringify({...example,id:crypto.randomUUID(),serverId},null,2))
    return ()=>{active=false;clearInterval(timer)}
  },[serverId])
  useEffect(()=>{
    if(!selected)return
    let active=true
    const load=()=>void invoke<Result[]>("synthetic_results",{id:selected}).then(rs=>{if(active)setHistory(rs)}).catch(()=>{if(active)setMessage("Could not load synthetic history")})
    load()
    const timer=setInterval(load,15000)
    return ()=>{active=false;clearInterval(timer)}
  },[selected])
  const action=async(run:boolean)=>{
    if(!writable)return
    setBusy(true)
    try {
      const workflow={...JSON.parse(draft),serverId}
      if(run){const result=await invoke<Result>("run_synthetic",{workflow});setHistory(h=>[...h,result].slice(-500));setSelected(workflow.id);setMessage(result.ok?"All steps passed":"A step assertion failed")}
      else {await invoke("save_document",{kind:"synthetic",document:workflow});setChecks(cs=>[...cs.filter(c=>c.id!==workflow.id),workflow]);setSelected(workflow.id);setMessage("Workflow saved. Checks run while Tracelyn is open")}
    }catch{setMessage("Check failed. Verify the workflow, connection, DNS and certificate trust")}
    finally{setBusy(false)}
  }
  const latest=history[history.length-1]
  return <section className="space-y-4 rounded-lg border border-border bg-card p-4 text-card-foreground">
    <h2 className="font-semibold">Synthetic checks · PC and VPS</h2>
    <p className="text-sm text-muted-foreground">HTTP, SSL and DNS probes use verified curl GET requests. Set vantage to pc or vps. Steps run in order and stop on failure. Extract response fields with JSON pointers such as /token, then reference variables as {"{{token}}"} in later URLs. Workflows support one to eight steps and intervals from 15 to 86400 seconds.</p>
    <p className="text-sm text-muted-foreground">DNS checks resolve directly from the selected vantage and can assert an address keyword. SSL checks require HTTPS and report certificate expiry when curl supports certificate metadata. Keep secrets out of saved URLs. Extracted variables stay in memory for the run.</p>
    <div className="flex flex-wrap gap-3">{checks.map((c,i)=><Button key={c.id} variant="outline" onClick={()=>{setSelected(c.id);setDraft(JSON.stringify(c,null,2));setHistory([])}}>Workflow {i+1} · {c.vantage.toUpperCase()}</Button>)}<Button variant="outline" disabled={!writable} onClick={()=>{setDraft(JSON.stringify({...example,id:crypto.randomUUID(),serverId},null,2));setSelected("");setHistory([])}}>New workflow</Button></div>
    <label className="block space-y-2"><span className="text-sm font-medium">Workflow JSON</span><textarea aria-label="Synthetic workflow JSON" className="h-64 w-full rounded border border-border bg-background p-3 font-mono text-sm text-foreground" readOnly={!writable} value={enabled?"Workflow hidden in privacy mode":draft} onChange={e=>setDraft(e.target.value)} /></label>
    <div className="flex gap-3"><Button disabled={!writable||busy} onClick={()=>void action(false)}>Save workflow</Button><Button variant="outline" disabled={!writable||busy} onClick={()=>void action(true)}>{busy?"Checking":"Run now"}</Button><Button variant="ghost" disabled={!writable||!selected||busy} onClick={async()=>{try{await invoke("delete_document",{kind:"synthetic",id:selected});setChecks(cs=>cs.filter(c=>c.id!==selected));setSelected("");setHistory([])}catch{setMessage("Could not remove workflow")}}}>Delete workflow</Button></div>
    {message&&<p role="status" className="text-sm">{message}</p>}
    <MetricUplot title="Synthetic response time in ms" points={history.map(r=>({t:r.tsMs,v:r.steps.reduce((n,s)=>n+s.totalMs,0)}))} />
    {latest&&<div className="space-y-2"><p className="text-sm">{latest.vantage.toUpperCase()} · {latest.ok?"Passed":"Failed"} · {new Date(latest.tsMs).toLocaleString()}</p>{latest.steps.map((s,i)=><p key={i} className="rounded border border-border p-3 text-sm">Step {i+1} · {s.message} · HTTP {s.status} · {s.totalMs.toFixed(0)} ms · DNS {s.dnsMs.toFixed(0)} ms · Certificate {s.certificateDays===null?"expiry unavailable":`${s.certificateDays} days remaining`}</p>)}</div>}
  </section>
}
