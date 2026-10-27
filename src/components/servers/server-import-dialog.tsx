import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Download, FileUp, Loader2 } from "lucide-react";
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
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { SimpleSelect } from "@/components/ui/select";
import { Notice } from "@/components/status/notice";
import { useServersContext } from "@/contexts/servers-context";
import { Masked, usePrivacy } from "@/contexts/privacy-context";
import { listSshPrivateKeys } from "@/lib/api";
import { previewServerImport } from "@/lib/fleet-api";
import {
  importDuplicates,
  importInput,
  parseServerCsv,
  type ImportPreview,
} from "@/lib/server-import";
import type { ServerRecord } from "@/lib/types";

export function ServerImportDialog({
  open,
  onOpenChange,
  onImported,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onImported?: (servers: ServerRecord[]) => void;
}) {
  const { servers, refresh, setSelectedServerId } = useServersContext();
  const { enabled } = usePrivacy();
  const [format, setFormat] = useState<"ssh" | "csv">("ssh");
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [selected, setSelected] = useState<number[]>([]);
  const [fallbackKey, setFallbackKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState("");
  const [completed, setCompleted] = useState<number[]>([]);
  const duplicates = useMemo(
    () => importDuplicates(preview?.servers ?? [], servers),
    [preview, servers],
  );
  const eligible = (preview?.servers ?? [])
    .map((row, i) => ({ row, i }))
    .filter(
      ({ row, i }) =>
        !row.issues.length && !duplicates.has(i) && !completed.includes(i),
    )
    .map(({ i }) => i);
  const picked = selected.filter((i) => eligible.includes(i));

  const load = async (local = false, raw = text) => {
    setBusy(true);
    setError(null);
    setProgress("");
    setPreview(null);
    setCompleted([]);
    try {
      if (new TextEncoder().encode(raw).length > 2_000_000)
        throw new Error("Import files must be smaller than 2 MB");
      const next =
        format === "csv"
          ? parseServerCsv(raw)
          : await previewServerImport("ssh", local ? undefined : raw);
      setPreview(next);
      const dupes = importDuplicates(next.servers, servers);
      setSelected(
        next.servers
          .map((_, i) => i)
          .filter((i) => !next.servers[i].issues.length && !dupes.has(i)),
      );
      if (next.servers.some((s) => s.authType === "key" && !s.keyPath)) {
        const keys = await listSshPrivateKeys().catch(() => null);
        setFallbackKey(keys?.keys[0] ?? "");
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };
  const importSelected = async () => {
    if (!preview) return;
    setBusy(true);
    setError(null);
    setProgress(`Importing ${picked.length} profiles…`);
    try {
      const outcome = await invoke<{
        imported: ServerRecord[];
        skipped: number;
      }>("import_server_profiles", {
        inputs: picked.map((i) =>
          importInput(preview.servers[i], fallbackKey || undefined),
        ),
      });
      setCompleted((previous) => [...previous, ...picked]);
      setSelected([]);
      setProgress(
        `${outcome.imported.length} imported${outcome.skipped ? `, ${outcome.skipped} duplicates skipped` : ""}`,
      );
      await refresh().catch(() =>
        setError(
          "Profiles were saved. Close this dialog and refresh the server list",
        ),
      );
      if (outcome.imported[0]) setSelectedServerId(outcome.imported[0].id);
      toast.success(
        `${outcome.imported.length} server${outcome.imported.length === 1 ? "" : "s"} imported`,
      );
      onImported?.(outcome.imported);
    } catch (err) {
      setError(String(err));
      setProgress("No profiles were imported. Fix the problem and retry");
    } finally {
      setBusy(false);
    }
  };
  const needsKey = picked.some(
    (i) =>
      preview?.servers[i].authType === "key" && !preview?.servers[i].keyPath,
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!busy) onOpenChange(next);
      }}
    >
      <DialogContent className="sm:max-w-4xl" showCloseButton={!busy}>
        <DialogHeader>
          <DialogTitle>Import servers</DialogTitle>
          <DialogDescription>
            Review profiles before adding them. Credentials are added later in
            Edit server. Existing profiles are kept.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-wrap items-end gap-3">
          <div className="min-w-44">
            <Label htmlFor="import-format">Import format</Label>
            <SimpleSelect
              id="import-format"
              value={format}
              onValueChange={(value) => {
                setFormat(value as "ssh" | "csv");
                setPreview(null);
                setText("");
                setError(null);
              }}
              options={[
                { value: "ssh", label: "SSH config" },
                { value: "csv", label: "CSV" },
              ]}
              disabled={busy}
            />
          </div>
          {format === "ssh" && (
            <Button
              variant="outline"
              onClick={() => void load(true)}
              disabled={busy}
            >
              <Download aria-hidden />
              Read ~/.ssh/config
            </Button>
          )}
          <div className="min-w-0 flex-1">
            <Label htmlFor="import-file">Choose a file</Label>
            <Input
              id="import-file"
              type="file"
              accept={format === "csv" ? ".csv,text/csv" : ".conf,.txt,*"}
              disabled={busy}
              onChange={(event) => {
                const file = event.target.files?.[0];
                if (!file) return;
                if (file.size > 2_000_000) {
                  setError("Import files must be smaller than 2 MB");
                  return;
                }
                void file
                  .text()
                  .then((raw) => {
                    setText(raw);
                    void load(false, raw);
                  })
                  .catch((err) => setError(String(err)));
              }}
            />
          </div>
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="import-text">
            Or paste {format === "ssh" ? "Host entries" : "CSV rows"}
          </Label>
          <textarea
            id="import-text"
            rows={4}
            value={text}
            disabled={busy}
            spellCheck={false}
            className={`w-full rounded-md border border-input bg-background px-3 py-2 font-mono text-xs focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${enabled ? "privacy-blur" : ""}`}
            onChange={(event) => {
              setText(event.target.value);
              setPreview(null);
            }}
          />
          {format === "csv" && (
            <p className="text-xs text-muted-foreground">
              Required columns: host, username. Optional: name, port, authType,
              keyPath, labels. Use | between labels, or a JSON list. Exported
              CSV can be imported here.
            </p>
          )}
          <Button
            className="w-fit"
            variant="outline"
            disabled={busy || !text.trim()}
            onClick={() => void load()}
          >
            {busy && !progress ? (
              <Loader2 className="animate-spin" aria-hidden />
            ) : (
              <FileUp aria-hidden />
            )}
            Preview import
          </Button>
        </div>
        {error && (
          <Notice tone="critical" title="Import needs attention">
            <Masked text={error} />
          </Notice>
        )}
        {preview && (
          <>
            {preview.warnings.length > 0 && (
              <Notice title="Review SSH and CSV settings">
                {preview.warnings.map((warning, i) => (
                  <p key={i}>
                    <Masked text={warning} />
                  </p>
                ))}
              </Notice>
            )}
            {preview.servers.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                No server entries found. SSH imports need a concrete Host alias.
                Wildcard entries provide defaults only.
              </p>
            ) : (
              <>
                <div className="flex flex-wrap items-center gap-3">
                  <label className="flex items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      className="size-4 accent-primary"
                      checked={
                        eligible.length > 0 && picked.length === eligible.length
                      }
                      disabled={busy || !eligible.length}
                      onChange={(event) =>
                        setSelected(event.target.checked ? eligible : [])
                      }
                    />
                    Select all available
                  </label>
                  <span className="text-xs text-muted-foreground" role="status">
                    {picked.length} selected · {duplicates.size} duplicates ·{" "}
                    {preview.servers.filter((s) => s.issues.length > 0).length}{" "}
                    need changes
                  </span>
                </div>
                <div
                  className="max-h-72 overflow-auto rounded-lg border border-border"
                  tabIndex={0}
                  role="region"
                  aria-label="Import preview"
                >
                  <table className="w-full min-w-[760px] text-left text-xs">
                    <caption className="sr-only">
                      Server import preview with duplicates and issues
                    </caption>
                    <thead className="sticky top-0 bg-muted">
                      <tr>
                        {[
                          "Import",
                          "Name",
                          "Host",
                          "User",
                          "Port",
                          "Identity file",
                          "ProxyJump",
                          "Review",
                        ].map((h) => (
                          <th scope="col" key={h} className="p-3 font-medium">
                            {h}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {preview.servers.map((row, i) => (
                        <tr key={i} className="border-t border-border">
                          <td className="p-3">
                            <input
                              type="checkbox"
                              className="size-4 accent-primary"
                              aria-label={`Import ${row.name}`}
                              checked={picked.includes(i)}
                              disabled={busy || !eligible.includes(i)}
                              onChange={(event) =>
                                setSelected((previous) =>
                                  event.target.checked
                                    ? [...previous, i]
                                    : previous.filter((n) => n !== i),
                                )
                              }
                            />
                          </td>
                          <td className="p-3 font-medium">{row.name}</td>
                          <td className="p-3">
                            <Masked text={row.host} />
                          </td>
                          <td className="p-3">
                            <Masked text={row.username} />
                          </td>
                          <td className="p-3">{row.port}</td>
                          <td className="max-w-48 break-all p-3">
                            <Masked
                              text={
                                row.keyPath ??
                                (row.authType === "key"
                                  ? "Use key below"
                                  : "Password needed later")
                              }
                            />
                          </td>
                          <td className="p-3">
                            <Masked text={row.proxyJump ?? "—"} />
                          </td>
                          <td className="max-w-60 p-3">
                            {completed.includes(i)
                              ? "Imported"
                              : duplicates.has(i)
                                ? `Duplicate of ${duplicates.get(i)}`
                                : row.issues.length
                                  ? row.issues.join(". ")
                                  : "Ready"}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </>
            )}
            {needsKey && (
              <div className="flex flex-col gap-2">
                <Label htmlFor="import-key">
                  Default private key for profiles without IdentityFile
                </Label>
                <Input
                  id="import-key"
                  value={fallbackKey}
                  onChange={(event) => setFallbackKey(event.target.value)}
                  disabled={busy}
                  placeholder="C:\Users\you\.ssh\id_ed25519"
                  className={enabled ? "privacy-blur" : ""}
                />
                <p className="text-xs text-muted-foreground">
                  Leave blank to configure a key later. Imported profiles are
                  not connected automatically.
                </p>
              </div>
            )}
          </>
        )}
        <p
          role="status"
          aria-live="polite"
          className="text-sm text-muted-foreground"
        >
          {progress || (busy ? "Reading server profiles…" : "")}
        </p>
        <DialogFooter>
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => onOpenChange(false)}
          >
            Close
          </Button>
          <Button
            disabled={busy || !picked.length}
            onClick={() => void importSelected()}
          >
            {busy && <Loader2 className="animate-spin" aria-hidden />}Import{" "}
            {picked.length || "selected"} server{picked.length === 1 ? "" : "s"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
