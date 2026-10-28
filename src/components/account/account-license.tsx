import { useState } from "react"
import { Link } from "react-router-dom"
import { openUrl } from "@tauri-apps/plugin-opener"
import { toast } from "sonner"
import { useLicense } from "@/contexts/license-context"
import { licenseMessage, showTrialBanner } from "@/lib/license"
import { useRememberedState } from "@/hooks/use-remembered-state"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import config from "../../../src-tauri/license-config.json"

function StoreLink({ kind, children }: { kind: "buy_url" | "manage_url", children: React.ReactNode }) {
  return <Button variant="outline" onClick={() => void openUrl(config[kind]).catch(() => toast.error("Could not open your browser"))}>{children}</Button>
}
export function LicenseBanner() {
  const { license, error } = useLicense()
  const [dismissed, setDismissed] = useRememberedState("trial-banner-dismissed", false)
  if (!error && (!license || (license.canWrite && !(showTrialBanner(license) && !dismissed) && license.status !== "Grace"))) return null
  return <aside role="status" className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-muted p-4 text-sm text-foreground">
    <p className="max-w-2xl">{error ?? (license && licenseMessage(license))}</p>
    <div className="flex items-center gap-3">
      <Link className="font-medium underline underline-offset-4" to="/settings#account-license">Account and License</Link>
      <StoreLink kind="buy_url">Upgrade</StoreLink>
      {license?.status === "Trial" && <Button variant="ghost" onClick={() => setDismissed(true)} aria-label="Dismiss trial reminder">Dismiss</Button>}
    </div>
  </aside>
}
export function AccountLicenseCard() {
  const { license, error, refresh, activate, deactivate } = useLicense()
  const [email, setEmail] = useRememberedState("account-email", "")
  const [open, setOpen] = useState(false)
  const [key, setKey] = useState("")
  const [activationError, setActivationError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  return <>
    <Card id="account-license" className="scroll-mt-6">
      <CardHeader><CardTitle>Account and License</CardTitle><CardDescription>Your license key is your account. No password or sign-in required.</CardDescription></CardHeader>
      <CardContent className="flex flex-col gap-4">
        <p role="status" className="text-sm">{error ?? (license ? licenseMessage(license) : "Reading your license…")}</p>
        {license && <dl className="grid grid-cols-2 gap-3 rounded-lg border border-border p-4 text-sm">
          <dt className="text-muted-foreground">Plan</dt><dd>{license.plan}</dd>
          <dt className="text-muted-foreground">Status</dt><dd>{license.status}{!license.canWrite && " · Read-only"}</dd>
          <dt className="text-muted-foreground">{license.status === "Trial" ? "Trial days left" : license.status === "Grace" ? "Grace days left" : "Plan days left"}</dt><dd>{license.daysLeft < 0 ? "No fixed expiry" : license.daysLeft}</dd>
          <dt className="text-muted-foreground">License key</dt><dd className="break-all font-mono">{license.maskedKey ?? "No key activated"}</dd>
          <dt className="text-muted-foreground">Device</dt><dd className="break-all">{license.device}</dd>
        </dl>}
        <label className="grid gap-2 text-sm">Account email (optional)
          <Input type="email" autoComplete="email" value={email} onChange={event => setEmail(event.target.value)} placeholder="you@example.com" />
        </label>
        <p className="text-xs text-muted-foreground">Stored locally on this device. Your email is not sent to the license service.</p>
        <div className="flex flex-wrap gap-2">
          {!license?.maskedKey && <Button onClick={() => setOpen(true)}>Activate license</Button>}
          {license?.maskedKey && <Button disabled={busy} variant="outline" onClick={async () => {
            setBusy(true)
            try { await deactivate(); toast.success("This device has been deactivated") }
            catch (error) { toast.error("Could not deactivate this device", { description: String(error) }) }
            finally { setBusy(false) }
          }}>Deactivate this device</Button>}
          <StoreLink kind="manage_url">Manage subscription</StoreLink>
          <StoreLink kind="buy_url">Buy a license</StoreLink>
          <Button variant="ghost" disabled={busy} onClick={() => void refresh()}>Refresh status</Button>
        </div>
      </CardContent>
    </Card>
    <Dialog open={open} onOpenChange={value => { if (!busy) { setOpen(value); setKey(""); setActivationError(null) } }}>
      <DialogContent>
        <DialogTitle>Activate your license</DialogTitle>
        <DialogDescription>Paste the license key from your purchase email to activate this device.</DialogDescription>
        <form className="grid gap-4" onSubmit={async event => {
          event.preventDefault()
          setBusy(true); setActivationError(null)
          try { await activate(key); setKey(""); setOpen(false); toast.success("License activated") }
          catch (error) { setActivationError(String(error)) }
          finally { setBusy(false) }
        }}>
          <label className="grid gap-2 text-sm">License key<Input autoFocus type="password" autoComplete="off" spellCheck={false} maxLength={256} value={key} onChange={event => setKey(event.target.value)} aria-describedby={activationError ? "activation-error" : undefined} /></label>
          {activationError && <p id="activation-error" role="alert" className="text-sm text-destructive">{activationError}</p>}
          <Button disabled={busy || !key.trim()} type="submit">{busy ? "Activating…" : "Activate this device"}</Button>
        </form>
      </DialogContent>
    </Dialog>
  </>
}
