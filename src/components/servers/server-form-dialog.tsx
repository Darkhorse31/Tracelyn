import { useEffect, useState } from "react";
import { open as openFile } from "@tauri-apps/plugin-dialog";
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
import { Masked, SecretInput, usePrivacy } from "@/contexts/privacy-context";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  SimpleSelect,
} from "@/components/ui/select";
import { inspectSshPrivateKey, listSshPrivateKeys } from "@/lib/api";
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
  const { enabled, field } = usePrivacy();
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
  const [sshDir, setSshDir] = useState("");
  const [detectedKeys, setDetectedKeys] = useState<string[]>([]);
  const [keyMessage, setKeyMessage] = useState<string | null>(null);

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
    setKeyMessage(null);
  }, [open, server]);

  useEffect(() => {
    if (!open || authType !== "key") return;
    void listSshPrivateKeys()
      .then((list) => {
        setSshDir(list.directory);
        setDetectedKeys(list.keys);
      })
      .catch(() => {
        setDetectedKeys([]);
      });
  }, [open, authType]);

  const checkKey = async (path: string) => {
    const trimmed = path.trim();
    if (!trimmed) {
      setKeyMessage(null);
      return true;
    }
    try {
      const result = await inspectSshPrivateKey(trimmed);
      setKeyMessage(result.ok ? null : result.message);
      return result.ok;
    } catch (err) {
      setKeyMessage(String(err));
      return false;
    }
  };

  const browseKey = async () => {
    const picked = await openFile({
      title: "Private key",
      directory: false,
      multiple: false,
      defaultPath: sshDir || undefined,
    });
    if (typeof picked === "string" && picked) {
      setKeyPath(picked);
      await checkKey(picked);
    }
  };

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

  const validate = async () => {
    if (!name.trim() || !host.trim() || !username.trim()) {
      toast.error("Name, host, and username are required.");
      return false;
    }
    if (authType === "key" && !keyPath.trim()) {
      toast.error("SSH key path is required for key authentication.");
      return false;
    }
    if (authType === "key" && !(await checkKey(keyPath))) {
      toast.error("Choose an OpenSSH private key.");
      return false;
    }
    if (authType === "password" && !password && !server) {
      toast.error("Password is required for new password-based servers.");
      return false;
    }
    return true;
  };

  const handleTest = async () => {
    if (!(await validate())) return;
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
    if (!(await validate())) return;
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
      <DialogContent className="sm:max-w-lg">
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
              <SecretInput
                id="server-host"
                kind="host"
                value={host}
                onChange={(e) => setHost(e.target.value)}
                placeholder="203.0.113.10"
                className="font-mono"
              />
            </div>
            <div className="flex flex-col gap-2">
              <Label htmlFor="server-port">Port</Label>
              <SecretInput
                id="server-port"
                kind="port"
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
              <SecretInput
                id="server-user"
                kind="user"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className="font-mono"
              />
            </div>
            <div className="flex flex-col gap-2 sm:col-span-2">
              <Label id="server-auth-label">Authentication</Label>
              <Select
                value={authType}
                onValueChange={(v) => setAuthType(v as AuthType)}
              >
                <SelectTrigger aria-labelledby="server-auth-label" className="w-full">
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
                  <div className="flex gap-2">
                    <SecretInput
                      id="server-key-path"
                      kind="path"
                      value={keyPath}
                      onChange={(e) => {
                        setKeyPath(e.target.value);
                        setKeyMessage(null);
                      }}
                      onBlur={() => void checkKey(keyPath)}
                      placeholder="C:\Users\you\.ssh\id_ed25519"
                      className="min-w-0 flex-1 font-mono text-xs"
                      aria-invalid={keyMessage ? true : undefined}
                      aria-describedby={keyMessage ? "server-key-error" : undefined}
                    />
                    <Button type="button" variant="outline" onClick={() => void browseKey()}>
                      Browse…
                    </Button>
                  </div>
                  <Label htmlFor="server-key-pick">Detected keys</Label>
                  <SimpleSelect
                    id="server-key-pick"
                    value={detectedKeys.includes(keyPath) ? keyPath : ""}
                    placeholder={
                      enabled
                        ? "Detected keys"
                        : detectedKeys.length
                          ? `Keys in ${sshDir || ".ssh"}`
                          : "No keys found in .ssh"
                    }
                    onValueChange={(next) => {
                      setKeyPath(next);
                      void checkKey(next);
                    }}
                    options={detectedKeys.map((file) => ({ value: file, label: field(file, "path") }))}
                  />
                  {keyMessage && (
                    <p id="server-key-error" role="alert" className="text-xs text-status-critical">
                      <Masked text={keyMessage} />
                    </p>
                  )}
                </div>
                <div className="flex flex-col gap-2 sm:col-span-2">
                  <Label htmlFor="server-passphrase">Key passphrase (optional)</Label>
                  <SecretInput
                    id="server-passphrase"
                    kind="secret"
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
                <SecretInput
                  id="server-password"
                  kind="secret"
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
