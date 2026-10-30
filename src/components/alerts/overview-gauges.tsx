import { Link } from "react-router-dom";
import { useEffect, useState } from "react";
import { useServerMetrics } from "@/hooks/use-server-metrics";
import { useWebTraffic } from "@/hooks/use-web-traffic";
import { useInfra, infraApi } from "@/components/infra/api";
import { StatCard } from "@/components/layout/enterprise";

export function OverviewGauges({ serverId, connected }: { serverId: string; connected: boolean }) {
  const { snapshot } = useServerMetrics(serverId, connected);
  const web = useWebTraffic(connected ? serverId : null);
  const infra = useInfra(serverId, "", false, true);
  const [uptime, setUptime] = useState<number | null>(null);
  useEffect(() => {
    let active = true;
    setUptime(null);
    const refresh = () => { void infraApi.checks(serverId).then(checks => {
      if (!active) return;
      const readings = checks.filter(c => c.check.enabled).flatMap(c=>c.history);
      setUptime(readings.length ? readings.filter(r=>r.ok).length/readings.length*100 : null);
    }).catch(() => { if (active) setUptime(null); }); };
    refresh(); const t = setInterval(refresh, 15000);
    return () => {active=false;clearInterval(t);};
  }, [serverId]);
  const certificates = (infra.data ?? []).filter(s => ["certificates", "certificates_pc"].includes(s.collector)).flatMap(s=>s.rows).map(r=>r.fields.daysRemaining).filter((v): v is number=>typeof v === "number" && Number.isFinite(v));
  const counts = Object.entries(web.metrics.statusBreakdown);
  const total = counts.reduce((sum,[,n])=>sum+n,0);
  const errors = counts.filter(([status])=>Number(status)>=500).reduce((sum,[,n])=>sum+n,0);
  return <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
    <StatCard label="Network received / sent" value={connected && snapshot ? `${(snapshot.netInBps/1024).toFixed(1)} / ${(snapshot.netOutBps/1024).toFixed(1)} KB/s` : "No reading"} />
    <StatCard label="Website requests" value={connected && web.webDetected && !web.loading ? `${web.metrics.reqPerSec.toFixed(1)} / second` : "No reading"} />
    <StatCard label="Website errors" value={connected && web.webDetected && total ? `${(errors/total*100).toFixed(1)}%` : "No reading"} />
    <StatCard label="Certificate time left" value={connected && certificates.length ? `${Math.floor(Math.min(...certificates))} days` : "No reading"} />
    <StatCard label="Website uptime" value={uptime === null ? "No reading" : `${uptime.toFixed(1)}%`} description="Successful checks in the saved availability history." />
    <div className="sm:col-span-2 xl:col-span-4 text-sm text-muted-foreground">These readings are for the selected server. <Link className="text-primary underline" to="/infra/certificates">Review availability history and certificate checks</Link>. Missing readings mean monitoring has not supplied that measurement.</div>
  </div>;
}
