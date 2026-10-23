import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
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
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { AuthType, ServerRecord } from "@/lib/types";
import { useServersContext } from "@/contexts/servers-context";

const defaultPort = 22;

function labelsFromString(raw: string): string[] {
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
}

interface ServerFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  server?: ServerRecord | null;
}

export function ServerFormDialog({
  open,
  onOpenChange,
  server,
}: ServerFormDialogProps) {
  const { saveServer, testConnection } = useServersContext();
  const [name, setName] = useState("");
  const [host, setHost] = useState("");
  const [port, setPort] = useState(String(defaultPort));
  const [username, setUsername] = useState("root");
  const [authType, setAuthType] = useState<AuthType>("key");
  const [keyPath, setKeyPath] = useState("");
  const [labelsRaw, setLabelsRaw] = useState("");
  const [password, setPassword] = useState("");
  const [keyPassphrase, setKeyPassphrase] = useState("");
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    if (!open) return;
    if (server) {
      setName(server.name);
      setHost(server.host);
      setPort(String(server.port));
      setUsername(server.username);
      setAuthType(server.authType);
      setKeyPath(server.keyPath ?? "");
      setLabelsRaw(server.labels.join(", "));
    } else {
      setName("");
      setHost("");
      setPort(String(defaultPort));
      setUsername("root");
      setAuthType("key");
      setKeyPath("");
      setLabelsRaw("");
    }
    setPassword("");
    setKeyPassphrase("");
  }, [open, server]);

  const buildInput = () => ({
    serverId: server?.id,
    name: name.trim(),
    host: host.trim(),
    port: Number(port) || defaultPort,
    username: username.trim(),
    authType,
    keyPath: authType === "key" ? keyPath.trim() : undefined,
    labels: labelsFromString(labelsRaw),
    password: authType === "password" ? password : undefined,
    keyPassphrase: authType === "key" ? keyPassphrase : undefined,
  });

  const validate = () => {
    if (!name.trim() || !host.trim() || !username.trim()) {
      toast.error("Name, host, and username are required.");
      return false;
    }
    if (authType === "key" && !keyPath.trim()) {
      toast.error("SSH key path is required for key authentication.");
      return false;
    }
    if (authType === "password" && !password && !server) {
      toast.error("Password is required for new password-based servers.");
      return false;
    }
    return true;
  };

  const handleTest = async () => {
    if (!validate()) return;
    setTesting(true);
    try {
      const result = await testConnection(buildInput());
      toast.success("Connection successful", {
        description: result.hostname ?? result.osInfo.slice(0, 120),
      });
    } catch (err) {
      toast.error("Connection failed", {
        description: String(err),
      });
    } finally {
      setTesting(false);
    }
  };

  const handleSave = async () => {
    if (!validate()) return;
    setSaving(true);
    try {
      const payload = buildInput();
      await saveServer({
        id: server?.id,
        name: payload.name,
        host: payload.host,
        port: payload.port,
        username: payload.username,
        authType: payload.authType,
        keyPath: payload.keyPath,
        labels: payload.labels,
        password: payload.password,
        keyPassphrase: payload.keyPassphrase,
      });
      toast.success(server ? "Server updated" : "Server added");
      onOpenChange(false);
    } catch (err) {
      toast.error("Could not save server", { description: String(err) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{server ? "Edit server" : "Add server"}</DialogTitle>
          <DialogDescription>
            Credentials are stored in Windows Credential Manager. Host keys use
            trust-on-first-use.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-4">
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="flex flex-col gap-2 sm:col-span-2">
              <Label htmlFor="server-name">Display name</Label>
              <Input
                id="server-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Production API"
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="server-host">Host</Label>
              <Input
                id="server-host"
                value={host}
                onChange={(e) => setHost(e.target.value)}
                placeholder="203.0.113.10"
                className="font-mono"
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="server-port">Port</Label>
              <Input
                id="server-port"
                type="number"
                min={1}
                max={65535}
                value={port}
                onChange={(e) => setPort(e.target.value)}
                className="font-mono font-tabular"
              />
            </div>
            <div className="flex flex-col gap-2 sm:col-span-2">
              <Label htmlFor="server-user">Username</Label>
              <Input
                id="server-user"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className="font-mono"
              />
            </div>
            <div className="flex flex-col gap-2 sm:col-span-2">
              <Label>Authentication</Label>
              <Select
                value={authType}
                onValueChange={(v) => setAuthType(v as AuthType)}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="key">SSH key</SelectItem>
                  <SelectItem value="password">Password</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {authType === "key" ? (
              <>
                <div className="flex flex-col gap-2 sm:col-span-2">
                  <Label htmlFor="server-key-path">Private key path</Label>
                  <Input
                    id="server-key-path"
                    value={keyPath}
                    onChange={(e) => setKeyPath(e.target.value)}
                    placeholder="C:\Users\you\.ssh\id_ed25519"
                    className="font-mono text-xs"
                  />
                </div>
                <div className="flex flex-col gap-2 sm:col-span-2">
                  <Label htmlFor="server-passphrase">Key passphrase (optional)</Label>
                  <Input
                    id="server-passphrase"
                    type="password"
                    value={keyPassphrase}
                    onChange={(e) => setKeyPassphrase(e.target.value)}
                    autoComplete="off"
                  />
                </div>
              </>
            ) : (
              <div className="flex flex-col gap-2 sm:col-span-2">
                <Label htmlFor="server-password">Password</Label>
                <Input
                  id="server-password"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder={server ? "Leave blank to keep stored password" : ""}
                  autoComplete="off"
                />
              </div>
            )}
            <div className="flex flex-col gap-2 sm:col-span-2">
              <Label htmlFor="server-labels">Labels (comma-separated)</Label>
              <Input
                id="server-labels"
                value={labelsRaw}
                onChange={(e) => setLabelsRaw(e.target.value)}
                placeholder="production, web"
              />
            </div>
          </div>
        </div>
        <DialogFooter className="gap-2 sm:justify-between">
          <Button
            type="button"
            variant="outline"
            onClick={handleTest}
            disabled={testing || saving}
          >
            {testing && <Loader2 className="animate-spin" data-icon="inline-start" />}
            Test connection
          </Button>
          <div className="flex gap-2">
            <Button
              type="button"
              variant="ghost"
              onClick={() => onOpenChange(false)}
            >
              Cancel
            </Button>
            <Button type="button" onClick={handleSave} disabled={saving || testing}>
              {saving && <Loader2 className="animate-spin" data-icon="inline-start" />}
              Save
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
