import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react"
import { invoke } from "@tauri-apps/api/core"
import type { LicenseSnapshot } from "@/lib/license"

const LicenseContext = createContext<{
  license: LicenseSnapshot | null
  error: string | null
  refresh: () => Promise<void>
  activate: (key: string) => Promise<void>
  deactivate: () => Promise<void>
}>({ license: null, error: null, refresh: async () => {}, activate: async () => {}, deactivate: async () => {} })

export function LicenseProvider({ children }: { children: ReactNode }) {
  const [license, setLicense] = useState<LicenseSnapshot | null>(null)
  const [error, setError] = useState<string | null>(null)
  const refresh = useCallback(async () => {
    try {
      setLicense(await invoke<LicenseSnapshot>("license_status"))
      setError(null)
    } catch {
      setError("Could not read your license. Monitoring remains available. Try again in Settings.")
      setLicense(null)
    }
  }, [])
  useEffect(() => {
    void refresh()
    const timer = window.setInterval(() => void refresh(), 30_000)
    return () => window.clearInterval(timer)
  }, [refresh])
  const activate = async (key: string) => {
    setLicense(await invoke<LicenseSnapshot>("license_activate", { key }))
    setError(null)
  }
  const deactivate = async () => {
    setLicense(await invoke<LicenseSnapshot>("license_deactivate"))
    setError(null)
  }
  return <LicenseContext.Provider value={{ license, error, refresh, activate, deactivate }}>{children}</LicenseContext.Provider>
}
export function useLicense() { return useContext(LicenseContext) }
