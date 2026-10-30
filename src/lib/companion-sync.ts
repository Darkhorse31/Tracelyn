import { invoke } from "@tauri-apps/api/core"
import { toast } from "sonner"
export async function syncCompanionConfig(): Promise<void> {
  try { await invoke("companion_sync_all") }
  catch { toast.warning("Saved locally. Companion configuration could not be synced. Enable Operations mode and retry Sync configuration on the server.") }
}
