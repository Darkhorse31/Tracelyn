import { AccountLicenseCard } from "@/components/account/account-license";
import { UpdatesCard } from "@/components/account/updates";
import { useEffect, useState } from "react";
import { Pencil, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { OperationsModeCard } from "@/components/ops/operations-mode-card";
import { ThemeRadioGroup } from "@/components/theme/theme-controls";
import { ServerFormDialog } from "@/components/servers/server-form-dialog";
import { ConnectionStatusBadge } from "@/components/status/connection-status-badge";
import { Button, buttonVariants } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Masked, PrivacyToggle } from "@/contexts/privacy-context";
import { useServersContext } from "@/contexts/servers-context";
import { openCrashLog, readCrashLog } from "@/lib/api";
import type { ServerRecord } from "@/lib/types";
import { Link } from "react-router-dom";
import { ServerImportDialog } from "@/components/servers/server-import-dialog";
import { exportServers } from "@/lib/fleet-api";
import { writePreference } from "@/hooks/use-remembered-state";

export function SettingsPage() {
  const { servers, statusById, loading, removeServer } = useServersContext();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editing, setEditing] = useState<ServerRecord | null>(null);
  const [crashLog, setCrashLog] = useState("");
  const [importOpen, setImportOpen] = useState(false);
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    readCrashLog()
      .then(setCrashLog)
      .catch(() => setCrashLog(""));
  }, []);

  const openCreate = () => {
    setEditing(null);
    setDialogOpen(true);
  };

  const openEdit = (server: ServerRecord) => {
    setEditing(server);
    setDialogOpen(true);
  };

  const handleDelete = async (server: ServerRecord) => {
    if (!confirm(`Remove ${server.name}? Stored credentials will be deleted.`)) {
      return;
    }
    try {
      await removeServer(server.id);
      toast.success("Server removed");
    } catch (err) {
      toast.error("Could not remove server", { description: String(err) });
    }
  };

  return (
    <div className="mx-auto flex w-full max-w-3xl flex-col gap-6">
      <AccountLicenseCard />
      <UpdatesCard />
      <Card>
        <CardHeader>
          <CardTitle>Getting started</CardTitle>
          <CardDescription>
            Connect a server, run access checks, and choose the applications to show.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-wrap gap-2">
          <Link
            className={buttonVariants()}
            to="/welcome"
            onClick={() => writePreference("wizard-step", 0)}
          >
            Open setup wizard
          </Link>
          <Button variant="outline" onClick={() => setImportOpen(true)}>
            Import servers
          </Button>
          <Button
            variant="outline"
            disabled={!servers.length || exporting}
            onClick={() => {
              setExporting(true);
              void exportServers()
                .then((saved) => {
                  if (saved) toast.success("Server profiles exported without secrets");
                })
                .catch((err) => toast.error("Could not export profiles", { description: String(err) }))
                .finally(() => setExporting(false));
            }}
          >
            Export server profiles
          </Button>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>Appearance</CardTitle>
          <CardDescription>
            Light, dark, or match your operating system. Applies to charts and
            the window title bar.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <ThemeRadioGroup />
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="max-w-md text-sm text-muted-foreground">
              Privacy mode hides addresses, usernames, keys, and secrets on screen and in copied text. Shortcut Ctrl+Shift+P.
            </p>
            <PrivacyToggle />
          </div>
        </CardContent>
      </Card>
      <OperationsModeCard />
      <Card>
        <CardHeader>
          <CardTitle>Diagnostics</CardTitle>
          <CardDescription>
            If the desktop process panics, a crash log is written on this PC. Nothing is uploaded.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => {
                openCrashLog().catch((err) =>
                  toast.error("Could not open the crash log", { description: String(err) }),
                );
              }}
            >
              Open crash log
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => {
                const text = crashLog.trim();
                if (!text) {
                  toast.message("No crash log yet");
                  return;
                }
                navigator.clipboard
                  .writeText(text)
                  .then(() => toast.success("Crash log copied"))
                  .catch((err) => toast.error("Could not copy the crash log", { description: String(err) }));
              }}
            >
              Copy crash log
            </Button>
          </div>
          <pre className="max-h-48 overflow-auto rounded-lg bg-muted p-3 font-mono text-xs whitespace-pre-wrap">
            {crashLog.trim() ? crashLog : "No crash log yet."}
          </pre>
        </CardContent>
      </Card>
      <Card>
        <CardHeader className="flex flex-row items-start justify-between gap-4">
          <div>
            <CardTitle>Servers</CardTitle>
            <CardDescription>
              Connection profiles without secrets. Passwords and passphrases live
              in Windows Credential Manager.
            </CardDescription>
          </div>
          <Button size="sm" onClick={openCreate}>
            <Plus data-icon="inline-start" />
            Add
          </Button>
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          {loading ? (
            <Skeleton className="h-24 w-full" />
          ) : servers.length === 0 ? (
            <p className="text-sm text-muted-foreground">No servers yet.</p>
          ) : (
            servers.map((server) => (
              <div
                key={server.id}
                className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-card px-3 py-3"
              >
                <div className="min-w-0">
                  <p className="font-medium">{server.name}</p>
                  <p className="font-mono text-xs text-muted-foreground">
                    <Masked text={`${server.username}@${server.host}:${server.port}`} />
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <ConnectionStatusBadge
                    status={statusById.get(server.id)?.status ?? "disconnected"}
                  />
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    onClick={() => openEdit(server)}
                    aria-label={`Edit ${server.name}`}
                  >
                    <Pencil />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    onClick={() => handleDelete(server)}
                    aria-label={`Delete ${server.name}`}
                  >
                    <Trash2 />
                  </Button>
                </div>
              </div>
            ))
          )}
        </CardContent>
      </Card>
      <ServerFormDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        server={editing}
      />
      <ServerImportDialog open={importOpen} onOpenChange={setImportOpen} />
    </div>
  );
}
