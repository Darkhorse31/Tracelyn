import { useCallback, useEffect, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { OpsConfirmDialog } from "@/components/ops/confirm-action-dialog"
import { HowToFix } from "@/components/setup/how-to-fix"
import { useServersContext } from "@/contexts/servers-context"
import { useLicense } from "@/contexts/license-context"
import { usePrivacy } from "@/contexts/privacy-context"
import type { ActionRequest, OpsVerb } from "@/lib/ops-types"

interface Status { state: string, version?: string, uptimeSecs?: number, lastHeartbeat?: number, diskUsed?: number }
export function CompanionCard({ serverId }: { serverId?: string | null }) {
  const { selectedServerId } = useServersContext()
  const selected = serverId === undefined ? selectedServerId : serverId
  const { license } = useLicense()
  const { mask, enabled: privacy } = usePrivacy()
  const [status, setStatus] = useState<Status | null>(null)
  const [error, setError] = useState("")
  const [request, setRequest] = useState<ActionRequest | null>(null)
  const [open, setOpen] = useState(false)
  const [logPaths, setLogPaths] = useState("")
  const refresh = useCallback(() => {
    if (!selected) return
    void invoke<Status>("companion_status", { serverId: selected }).then(value => { setStatus(value); setError("") }).catch(() => {setStatus(null); setError("Could not read companion status. Connect the server and check sudo access.")})
  }, [selected])
  useEffect(() => {setStatus(null); setError(""); refresh(); const timer = window.setInterval(refresh, 30000); return () => window.clearInterval(timer)}, [refresh])
  useEffect(() => {
    setLogPaths("")
    if (!selected) return
    let active = true
    void invoke<string[]>("companion_log_paths", { serverId: selected }).then(paths => {if (active) setLogPaths(paths.join("\n"))}).catch(() => {})
    return () => {active = false}
  }, [selected])
  const act = (verb: OpsVerb) => {
    if (!selected) return
    setRequest({ serverId: selected, kind: "companion", verb, target: "tracelyn-companion", useSudo: true, logPaths: verb === "configure" ? logPaths.split("\n").map(path => path.trim()).filter(Boolean) : [] })
    setOpen(true)
  }
  return <Card>
    <CardHeader>
      <CardTitle>Server companion</CardTitle>
      <CardDescription>Optional history and alerts that keep running when the desktop app is closed.</CardDescription>
    </CardHeader>
    <CardContent className="space-y-4">
      <p className="text-sm">Data stays on your server. The companion opens no TCP ports. Outbound checks and notifications contact only the destinations you configure.</p>
      {!selected ? <p className="text-sm">Select a server to manage its companion.</p> : <>
        <div className="grid gap-4 sm:grid-cols-2">
          <div><p className="text-sm text-muted-foreground">Status</p><p className="font-medium" role="status">{status?.state ?? (error ? "Unavailable" : "Checking")}</p></div>
          <div><p className="text-sm text-muted-foreground">Version</p><p>{status?.version ?? "Unknown"}</p></div>
          <div><p className="text-sm text-muted-foreground">Uptime</p><p>{status?.uptimeSecs === undefined ? "Unknown" : `${Math.floor(status.uptimeSecs / 60)} minutes`}</p></div>
          <div><p className="text-sm text-muted-foreground">Disk used</p><p>{status?.diskUsed === undefined ? "Unknown" : `${(status.diskUsed / 1048576).toFixed(1)} MB`}</p></div>
          <div className="sm:col-span-2"><p className="text-sm text-muted-foreground">Last heartbeat</p><p>{status?.lastHeartbeat ? new Date(status.lastHeartbeat).toLocaleString() : "Unknown"}</p></div>
        </div>
        {error && <p role="alert" className="text-sm text-destructive">{mask(error)}</p>}
        <div className="flex flex-wrap gap-2">
          {(["install", "update", "restart", "stop", "uninstall", "sync"] as const).map(verb => <Button key={verb} variant="outline" disabled={!license?.canWrite || !selected} onClick={() => act(verb)}>{verb === "sync" ? "Sync configuration" : verb[0].toUpperCase() + verb.slice(1)}</Button>)}
          <Button variant="outline" onClick={refresh}>Refresh</Button>
        </div>
        {!license?.canWrite && <p className="text-sm text-muted-foreground">A writable license is required for install and control actions.</p>}
        <div className="space-y-2">
          <label htmlFor={`companion-logs-${selected}`} className="block text-sm font-medium">Additional log files</label>
          <p className="text-sm text-muted-foreground">Journald is watched automatically. Add one absolute file path per line. The tracelyn user needs read access.</p>
          <textarea id={`companion-logs-${selected}`} rows={3} className="w-full rounded-lg border border-border bg-background p-3 font-mono text-sm text-foreground" disabled={privacy || !license?.canWrite} value={privacy ? "Paths hidden in privacy mode" : logPaths} onChange={event => setLogPaths(event.target.value)} />
          <Button variant="outline" disabled={privacy || !license?.canWrite} onClick={() => act("configure")}>Review log configuration</Button>
        </div>
      </>}
      <details className="rounded-lg border border-border p-4 text-sm">
        <summary className="cursor-pointer font-medium">Setup guide and sudo requirements</summary>
        <div className="mt-4 space-y-4">
          <p>Enable Operations mode in Settings. Each action opens a dry-run preview of the commands and asks you to confirm before execution.</p>
          <p>Installation needs a Linux server with systemd and sudo. Set up monitoring asks for the administrator password in the app and grants read access. The companion group can read monitoring data. It cannot read channel secrets.</p>
          <HowToFix text="companion log access" />
          <p>The service runs as tracelyn with access to adm and systemd-journal. Channel secrets are stored in a mode 600 file owned by tracelyn. Uninstall preserves history and configuration for recovery.</p>
          <p>Metrics are sampled every 5 seconds and top processes every 30 seconds. Raw samples remain for 48 hours and minute summaries for 30 days. Memory is capped at 64 MB. CPU, disk usage, and outbound traffic depend on configured checks and log volume.</p>
          <p>Linux binaries are optional desktop resources. If a matching binary is missing, Install shows the build command and monitoring continues over SSH.</p>
        </div>
      </details>
      <OpsConfirmDialog open={open} request={request} onOpenChange={value => {setOpen(value); if (!value) refresh()}} />
    </CardContent>
  </Card>
}
