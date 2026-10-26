import { useEffect, useId, useState } from "react";
import { Pencil, Send, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { EmptyState, Notice } from "@/components/status/notice";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
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
import { SimpleSelect } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { deleteChannel, listChannels, testChannel, upsertChannel } from "@/pages/intel/api";
import {
  CHANNEL_LABELS,
  emptyChannelConfig,
  type Channel,
  type ChannelKind,
  type ChannelWrite,
} from "@/pages/intel/types";

const KINDS = Object.keys(CHANNEL_LABELS) as ChannelKind[];

function needsSecret(kind: ChannelKind) {
  return kind !== "windows";
}

export function ChannelsPanel() {
  const [channels, setChannels] = useState<Channel[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<ChannelWrite | null>(null);
  const [testing, setTesting] = useState<string | null>(null);

  const load = () => {
    listChannels()
      .then((next) => {
        setChannels(next);
        setError(null);
      })
      .catch((err) => setError(String(err)));
  };

  useEffect(() => {
    load();
  }, []);

  return (
    <Card>
      <CardHeader className="flex flex-row items-start justify-between gap-3">
        <div>
          <CardTitle className="text-base">Channels</CardTitle>
          <CardDescription>
            Windows notifications, Slack, Discord, Teams, Telegram, SMTP email, and a generic webhook.
            URLs, bot tokens, and SMTP passwords are stored in Windows Credential Manager, not in the
            config file.
          </CardDescription>
        </div>
        <Button
          type="button"
          size="sm"
          onClick={() =>
            setEditing({ name: "", kind: "windows", enabled: true, config: emptyChannelConfig() })
          }
        >
          New channel
        </Button>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {error && (
          <Notice tone="critical" title="Could not load channels">
            {error}
          </Notice>
        )}
        {channels === null && !error && <Skeleton className="h-24 w-full" />}
        {channels && channels.length === 0 && (
          <EmptyState title="No channels" description="Add a channel, then use Test to confirm delivery." />
        )}
        {channels && channels.length > 0 && (
          <ul className="flex flex-col gap-2">
            {channels.map((channel) => (
              <li key={channel.id} className="flex flex-wrap items-center gap-2 rounded-lg border border-border px-3 py-2">
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium">{channel.name}</p>
                  <p className="text-xs text-muted-foreground">
                    {CHANNEL_LABELS[channel.kind]}
                    {channel.kind !== "windows" && (channel.hasSecret ? " · secret saved" : " · secret missing")}
                    {!channel.enabled && " · disabled"}
                  </p>
                </div>
                {!channel.enabled && <Badge variant="outline">Off</Badge>}
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={testing === channel.id}
                  onClick={async () => {
                    setTesting(channel.id);
                    try {
                      toast.success(await testChannel(channel.id));
                    } catch (err) {
                      toast.error(String(err));
                    } finally {
                      setTesting(null);
                    }
                  }}
                >
                  <Send aria-hidden /> Test
                </Button>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label={`Edit ${channel.name}`}
                  onClick={() =>
                    setEditing({
                      id: channel.id,
                      name: channel.name,
                      kind: channel.kind,
                      enabled: channel.enabled,
                      config: channel.config,
                    })
                  }
                >
                  <Pencil aria-hidden />
                </Button>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label={`Delete ${channel.name}`}
                  onClick={async () => {
                    try {
                      await deleteChannel(channel.id);
                      load();
                    } catch (err) {
                      toast.error(String(err));
                    }
                  }}
                >
                  <Trash2 aria-hidden />
                </Button>
              </li>
            ))}
          </ul>
        )}
        {editing && (
          <ChannelDialog
            initial={editing}
            onClose={() => setEditing(null)}
            onSaved={() => {
              setEditing(null);
              load();
            }}
          />
        )}
      </CardContent>
    </Card>
  );
}

function ChannelDialog({
  initial,
  onClose,
  onSaved,
}: {
  initial: ChannelWrite;
  onClose: () => void;
  onSaved: () => void;
}) {
  const formId = useId();
  const [draft, setDraft] = useState(initial);
  const [secret, setSecret] = useState("");
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const secretLabel =
    draft.kind === "telegram"
      ? "Bot token"
      : draft.kind === "email"
        ? "SMTP password"
        : "Webhook URL";

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{initial.id ? "Edit channel" : "New channel"}</DialogTitle>
          <DialogDescription>
            Secrets are written to Credential Manager and are not shown again. Leave the secret blank
            when editing to keep the saved value.
          </DialogDescription>
        </DialogHeader>
        <form
          id={formId}
          className="grid gap-3"
          onSubmit={async (event) => {
            event.preventDefault();
            setSaving(true);
            setFormError(null);
            try {
              await upsertChannel({ ...draft, secret: secret || undefined });
              toast.success("Channel saved");
              onSaved();
            } catch (err) {
              setFormError(String(err));
            } finally {
              setSaving(false);
            }
          }}
        >
          {formError && (
            <Notice tone="critical" title="Could not save">
              {formError}
            </Notice>
          )}
          <div className="grid gap-1.5">
            <Label htmlFor={`${formId}-name`}>Name</Label>
            <Input id={`${formId}-name`} value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} required />
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor={`${formId}-kind`}>Kind</Label>
            <SimpleSelect
              id={`${formId}-kind`}
              value={draft.kind}
              onValueChange={(next) => setDraft({ ...draft, kind: next as ChannelKind })}
              options={KINDS.map((kind) => ({ value: kind, label: CHANNEL_LABELS[kind] }))}
            />
          </div>
          {draft.kind === "telegram" && (
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-chat`}>Chat id</Label>
              <Input
                id={`${formId}-chat`}
                className="font-mono"
                value={draft.config.chatId}
                onChange={(e) => setDraft({ ...draft, config: { ...draft.config, chatId: e.target.value } })}
              />
            </div>
          )}
          {draft.kind === "email" && (
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="grid gap-1.5 sm:col-span-2">
                <Label htmlFor={`${formId}-host`}>SMTP host</Label>
                <Input
                  id={`${formId}-host`}
                  value={draft.config.smtpHost}
                  onChange={(e) => setDraft({ ...draft, config: { ...draft.config, smtpHost: e.target.value } })}
                />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor={`${formId}-port`}>Port</Label>
                <Input
                  id={`${formId}-port`}
                  type="number"
                  className="font-mono"
                  value={draft.config.smtpPort}
                  onChange={(e) => setDraft({ ...draft, config: { ...draft.config, smtpPort: Number(e.target.value) } })}
                />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor={`${formId}-tls`}>Security</Label>
                <SimpleSelect
                  id={`${formId}-tls`}
                  value={draft.config.smtpTls}
                  onValueChange={(next) => setDraft({ ...draft, config: { ...draft.config, smtpTls: next } })}
                  options={[
                    { value: "starttls", label: "STARTTLS" },
                    { value: "implicit", label: "Implicit TLS (465)" },
                    { value: "none", label: "None" },
                  ]}
                />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor={`${formId}-from`}>From</Label>
                <Input
                  id={`${formId}-from`}
                  type="email"
                  value={draft.config.mailFrom}
                  onChange={(e) => setDraft({ ...draft, config: { ...draft.config, mailFrom: e.target.value } })}
                />
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor={`${formId}-to`}>To</Label>
                <Input
                  id={`${formId}-to`}
                  type="email"
                  value={draft.config.mailTo}
                  onChange={(e) => setDraft({ ...draft, config: { ...draft.config, mailTo: e.target.value } })}
                />
              </div>
              <div className="grid gap-1.5 sm:col-span-2">
                <Label htmlFor={`${formId}-user`}>Username</Label>
                <Input
                  id={`${formId}-user`}
                  autoComplete="off"
                  value={draft.config.username}
                  onChange={(e) => setDraft({ ...draft, config: { ...draft.config, username: e.target.value } })}
                />
              </div>
            </div>
          )}
          {needsSecret(draft.kind) && (
            <div className="grid gap-1.5">
              <Label htmlFor={`${formId}-secret`}>{secretLabel}</Label>
              <Input
                id={`${formId}-secret`}
                type="password"
                autoComplete="new-password"
                spellCheck={false}
                value={secret}
                onChange={(e) => setSecret(e.target.value)}
                placeholder={initial.id ? "Leave blank to keep the saved secret" : ""}
              />
            </div>
          )}
          <label className="flex w-fit items-center gap-2 text-sm">
            <input
              type="checkbox"
              className="size-4 accent-primary"
              checked={draft.enabled}
              onChange={(e) => setDraft({ ...draft, enabled: e.target.checked })}
            />
            Enabled
          </label>
        </form>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" form={formId} disabled={saving || !draft.name.trim()}>
            Save channel
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
