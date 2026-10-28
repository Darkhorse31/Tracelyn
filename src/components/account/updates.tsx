import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react"
import { check, type Update } from "@tauri-apps/plugin-updater"
import { relaunch } from "@tauri-apps/plugin-process"
import { toast } from "sonner"
import { useRememberedState } from "@/hooks/use-remembered-state"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Switch } from "@/components/ui/switch"

const Updates = createContext({ autoCheck: true, setAutoCheck: (_: boolean) => {}, checkNow: async (_manual: boolean) => {}, checking: false })
function UpdateNotice({ update, onDone }: { update: Update, onDone: () => void }) {
  const [phase, setPhase] = useState<"ready" | "installing" | "installed">("ready")
  const [error, setError] = useState<string | null>(null)
  return <div className="grid w-full gap-3 rounded-lg border border-border bg-popover p-4 text-popover-foreground shadow-lg" role="status">
    <p className="font-semibold">Update available · {update.version}</p>
    <p className="max-h-40 overflow-auto whitespace-pre-wrap text-sm">{update.body || "A new version of Tracelyn is ready."}</p>
    {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
    <div className="flex flex-wrap gap-2">
      <Button disabled={phase === "installing"} onClick={async () => {
        setError(null)
        if (phase === "installed") {
          try { await relaunch() } catch { setError("Could not restart. Close and reopen Tracelyn to finish the update.") }
          return
        }
        setPhase("installing")
        try { await update.downloadAndInstall() }
        catch { setError("Could not finish the update. Check your connection and try again. The update must have a valid signature."); setPhase("ready"); return }
        setPhase("installed")
        try { await relaunch() } catch { setError("Could not restart. Close and reopen Tracelyn to finish the update.") }
      }}>{phase === "installing" ? "Installing…" : phase === "installed" ? "Restart now" : "Install and Restart"}</Button>
      <Button variant="ghost" disabled={phase === "installing"} onClick={onDone}>Later</Button>
    </div>
  </div>
}
export function UpdatesProvider({ children }: { children: ReactNode }) {
  const [autoCheck, setAutoCheck] = useRememberedState("auto-check-updates", true)
  const [checking, setChecking] = useState(false)
  const inFlight = useRef(false)
  const current = useRef<Update | null>(null)
  const checkNow = useCallback(async (manual: boolean) => {
    if (inFlight.current) return
    inFlight.current = true; setChecking(true)
    try {
      const update = await check({ timeout: 15000 })
      if (update) {
        await current.current?.close()
        current.current = update
        toast.custom(id => <UpdateNotice update={update} onDone={() => toast.dismiss(id)} />, { id: "app-update", duration: Infinity })
      } else if (manual) toast.success("You are using the latest version")
    } catch {
      if (manual) toast.error("Could not check for updates", { description: "Check your connection and try again. Release updates may not be configured yet." })
    } finally { inFlight.current = false; setChecking(false) }
  }, [])
  useEffect(() => {
    if (!autoCheck) return
    void checkNow(false)
    const timer = window.setInterval(() => void checkNow(false), 24 * 60 * 60 * 1000)
    return () => window.clearInterval(timer)
  }, [autoCheck, checkNow])
  useEffect(() => () => { void current.current?.close() }, [])
  return <Updates.Provider value={{ autoCheck, setAutoCheck, checkNow, checking }}>{children}</Updates.Provider>
}
export function UpdatesCard() {
  const { autoCheck, setAutoCheck, checkNow, checking } = useContext(Updates)
  return <Card><CardHeader><CardTitle>Updates</CardTitle><CardDescription>Updates are verified with the Tracelyn signing key before installation.</CardDescription></CardHeader>
    <CardContent className="flex flex-col gap-4">
      <div className="flex min-h-8 items-center justify-between gap-3">
        <span id="auto-check-updates-label" className="text-sm">Automatically check for updates</span>
        <Switch checked={autoCheck} onCheckedChange={setAutoCheck} aria-labelledby="auto-check-updates-label" />
      </div>
      <Button className="w-fit" variant="outline" disabled={checking} onClick={() => void checkNow(true)}>{checking ? "Checking…" : "Check for updates"}</Button>
    </CardContent></Card>
}
