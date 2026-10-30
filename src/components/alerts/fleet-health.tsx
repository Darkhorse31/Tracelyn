import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { useServersContext } from "@/contexts/servers-context";
import { usePrivacy } from "@/contexts/privacy-context";
import { listIncidents } from "@/pages/intel/api";
import type { Incident } from "@/pages/intel/types";
import { Section, StatCard, StatusBadge, ErrorState, Skeleton } from "@/components/layout/enterprise";
import { healthScore, nextStep } from "@/lib/incident-health";

export function FleetHealth() {
  const { servers, statusById } = useServersContext();
  const { mask, enabled } = usePrivacy();
  const [rows, setRows] = useState<Incident[]>([]);
  const [failed, setFailed] = useState(false);
  const [loading, setLoading] = useState(true);
  const refresh = () => { void listIncidents().then(v => {setRows(v);setFailed(false);}).catch(() => setFailed(true)).finally(() => setLoading(false)); };
  useEffect(() => { refresh(); const t = setInterval(refresh, 5000); return () => clearInterval(t); }, []);
  const active = rows.filter(i => i.status !== "resolved").sort((a,b) => ({critical:0,warning:1,info:2}[a.severity as "critical"] ?? 3) - ({critical:0,warning:1,info:2}[b.severity as "critical"] ?? 3));
  const connected = servers.filter(s => statusById.get(s.id)?.status === "connected").length;
  if (loading) return <Skeleton className="h-48 w-full" />;
  return <>
    <div className="grid gap-4 sm:grid-cols-3"><StatCard label="Servers connected" value={`${connected} / ${servers.length}`} /><StatCard label="Active issues" value={failed ? "Unavailable" : active.length} /><StatCard label="Fleet health" value={failed || !connected ? "Not measured" : `${Math.round(servers.reduce((sum,s) => sum + healthScore(statusById.get(s.id)?.status === "connected", active.filter(i=>i.serverId===s.id)),0)/Math.max(servers.length,1))} / 100`} description="Each server starts at 100. Each critical issue subtracts 40, each warning subtracts 15, and each other issue subtracts 5. A lost connection scores 0." /></div>
    <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">{servers.map((server,index) => {
      const issues = active.filter(i => i.serverId === server.id);
      const status = statusById.get(server.id)?.status;
      const online = status === "connected";
      const score = healthScore(online, issues);
      return <Section key={server.id} title={enabled ? `Server ${index+1}` : mask(server.name)}><StatusBadge status={failed || !online ? "unknown" : issues.some(i=>i.severity === "critical") ? "critical" : issues.length ? "warning" : "healthy"} /><p className="text-2xl font-semibold">{failed || !online ? "Not measured" : `${score} / 100`}</p><p className="text-sm text-muted-foreground">{!online ? "Connect this server to check its current health." : issues.length ? `${issues.length} active issues need your attention.` : "No active incidents detected."}</p><Link className="text-sm text-primary underline" to="/incidents">Review incidents and next steps</Link></Section>;
    })}</div>
    <Section title="What is wrong">
      {failed ? <ErrorState retry={refresh} /> : <>
        {active.length === 0 && <p className="text-sm">{connected === servers.length && connected > 0 ? "No active incidents have been detected. Connected servers are being monitored." : "Connect your servers to check their health. No active incidents have been recorded."}</p>}
        {active.map(i => <div key={i.id} className="flex flex-wrap items-start justify-between gap-4 border-b pb-4 last:border-0"><div className="space-y-2"><StatusBadge status={i.severity === "critical" ? "critical" : "warning"} /><p className="font-medium">{enabled ? "Issue on a monitored server" : mask(i.title)}</p><p className="text-sm text-muted-foreground">Since {new Date(i.openedAtMs).toLocaleString()}</p><p className="text-sm"><strong>What to do: </strong>{nextStep(i.kind)}</p></div><Link className="text-sm font-medium text-primary underline" to={`/intel/incidents/${i.id}`}>Open incident</Link></div>)}
      </>}
    </Section>
  </>;
}
