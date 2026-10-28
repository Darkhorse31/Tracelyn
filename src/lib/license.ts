export type LicenseStatus = "Trial" | "Active" | "Expired" | "Grace" | "Invalid"
export interface LicenseSnapshot {
  status: LicenseStatus
  canWrite: boolean
  daysLeft: number
  maskedKey: string | null
  device: string
  plan: string
  error: string | null
}
export function licenseMessage(license: LicenseSnapshot): string {
  if (license.error) return license.error
  if (license.status === "Trial") return `${license.daysLeft} day${license.daysLeft === 1 ? "" : "s"} left in your trial`
  if (license.status === "Grace") return "Offline grace period. Connect to the internet within 7 days of your last validation."
  if (license.status === "Active") return "Your license is active on this device."
  if (license.status === "Invalid") return "Your license is invalid or your device clock changed. Check your clock or activate a valid key."
  return "Your trial or offline grace period has ended. Monitoring remains available. Activate a license to enable actions and alerts."
}
export function showTrialBanner(license: LicenseSnapshot): boolean {
  return license.status === "Trial" && license.daysLeft <= 3
}
