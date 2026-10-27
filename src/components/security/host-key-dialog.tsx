import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { trustHostKey } from "@/lib/api";
import { listenWhileMounted } from "@/lib/events";

export interface HostKeyPrompt {
  host: string;
  port: number;
  fingerprint: string;
  changed: boolean;
  expected?: string | null;
}

export function parseHostKeyPrompt(error: unknown): HostKeyPrompt | null {
  const text = String(error);
  const marker = "HOSTKEY:";
  const at = text.indexOf(marker);
  if (at < 0) return null;
  try {
    const parsed = JSON.parse(text.slice(at + marker.length)) as HostKeyPrompt;
    if (!parsed?.fingerprint || !parsed.host) return null;
    return parsed;
  } catch {
    return null;
  }
}

export function reportHostKeyError(error: unknown): boolean {
  const prompt = parseHostKeyPrompt(error);
  if (!prompt) return false;
  window.dispatchEvent(new CustomEvent("tracelyn-host-key", { detail: prompt }));
  return true;
}

export function HostKeyDialog() {
  const [prompt, setPrompt] = useState<HostKeyPrompt | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(
    () =>
      listenWhileMounted<HostKeyPrompt>("host-key-prompt", (event) => {
        setPrompt(event.payload);
      }),
    [],
  );

  useEffect(() => {
    const onCustom = (event: Event) => {
      const detail = (event as CustomEvent<HostKeyPrompt>).detail;
      if (detail?.fingerprint) setPrompt(detail);
    };
    window.addEventListener("tracelyn-host-key", onCustom);
    return () => window.removeEventListener("tracelyn-host-key", onCustom);
  }, []);

  const trust = async () => {
    if (!prompt) return;
    setBusy(true);
    try {
      await trustHostKey(prompt.host, prompt.port, prompt.fingerprint);
      toast.success(prompt.changed ? "New host key saved" : "Host key trusted");
      setPrompt(null);
    } catch (err) {
      toast.error("Could not save the host key", { description: String(err) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={prompt != null} onOpenChange={(open) => !open && setPrompt(null)}>
      <DialogContent className={prompt?.changed ? "border-status-critical" : undefined}>
        <DialogHeader>
          <DialogTitle>{prompt?.changed ? "SSH host key changed" : "Trust this SSH host key?"}</DialogTitle>
          <DialogDescription>
            {prompt?.changed
              ? "The key presented by this server does not match the one you trusted. Connecting is blocked until you choose Trust or Cancel. Only continue if you expected the server to be rebuilt."
              : "Tracelyn has not seen this server before. Compare the SHA256 fingerprint with the host, then choose Trust or Cancel."}
          </DialogDescription>
        </DialogHeader>
        {prompt && (
          <div className="flex flex-col gap-2 text-sm">
            <p>
              {prompt.host}:{prompt.port}
            </p>
            <p className="rounded-lg bg-muted px-3 py-2 font-mono text-xs break-all">{prompt.fingerprint}</p>
            {prompt.changed && prompt.expected && (
              <p className="text-xs text-muted-foreground">
                Previously trusted
                <span className="mt-1 block font-mono break-all text-foreground">{prompt.expected}</span>
              </p>
            )}
          </div>
        )}
        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => setPrompt(null)}>
            Cancel
          </Button>
          <Button type="button" variant={prompt?.changed ? "destructive" : "default"} disabled={busy} onClick={() => void trust()}>
            Trust
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
