import { useEffect, useId, useState } from "react";
import { Send, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { EmptyState, Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { SimpleSelect } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { formatTs } from "@/components/intel/format";
import { deleteDigest, listChannels, listDigests, sendDigest, upsertDigest } from "@/pages/intel/api";
import type { Channel, DigestSpec } from "@/pages/intel/types";

const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

function blank(channelId: string): DigestSpec {
  return {
    id: "",
    name: "Morning digest",
    cadence: "daily",
    hourUtc: 8,
    weekday: 1,
    channelId,
    enabled: true,
  };
}

export function DigestPanel() {
  const [rows, setRows] = useState<DigestSpec[] | null>(null);
  const [channels, setChannels] = useState<Channel[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<DigestSpec | null>(null);
  const formId = useId();

  const load = () => {
    Promise.all([listDigests(), listChannels()])
      .then(([digests, nextChannels]) => {
        setRows(digests);
        setChannels(nextChannels);
        setError(null);
        setDraft((current) => current ?? blank(nextChannels[0]?.id ?? ""));
      })
      .catch((err) => setError(String(err)));
  };

  useEffect(() => {
    load();
  }, []);

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Digest</CardTitle>
        <CardDescription>
          A daily or weekly summary of incidents and anomalies. The scheduler sends it during the
          chosen UTC hour, and retries an hour later if delivery fails.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {error && (
          <Notice tone="critical" title="Could not load digests">
            {error}
          </Notice>
        )}
        {rows === null && !error && <Skeleton className="h-24 w-full" />}
        {rows && rows.length === 0 && <EmptyState title="No digest yet" description="Pick a channel and a time below." />}
        {rows && rows.length > 0 && (
          <ul className="flex flex-col gap-2">
            {rows.map((digest) => (
              <li key={digest.id} className="flex flex-wrap items-center gap-2 rounded-lg border border-border px-3 py-2">
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium">
                    {digest.name} {!digest.enabled && <span className="text-muted-foreground">(paused)</span>}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {digest.cadence === "weekly" ? `${WEEKDAYS[digest.weekday]} ` : "Every day "}
                    at {String(digest.hourUtc).padStart(2, "0")}:00 UTC
                    {digest.lastSentMs ? ` · last sent ${formatTs(digest.lastSentMs)}` : ""}
                    {digest.lastError ? ` · ${digest.lastError}` : ""}
                  </p>
                </div>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={async () => {
                    try {
                      toast.success(await sendDigest(digest.id));
                      load();
                    } catch (err) {
                      toast.error(String(err));
                      load();
                    }
                  }}
                >
                  <Send aria-hidden /> Send now
                </Button>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label={`Delete ${digest.name}`}
                  onClick={async () => {
                    try {
                      await deleteDigest(digest.id);
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
        {draft && (
          <form
            className="grid gap-3 rounded-lg border border-border p-3 md:grid-cols-2"
            onSubmit={async (event) => {
              event.preventDefault();
              try {
                await upsertDigest(draft);
                toast.success("Digest saved");
                setDraft(blank(draft.channelId));
                load();
              } catch (err) {
                toast.error(String(err));
              }
            }}
          >
            <div className="grid gap-2">
              <Label htmlFor={`${formId}-name`}>Name</Label>
              <Input id={`${formId}-name`} value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} required />
            </div>
            <div className="grid gap-2">
              <Label htmlFor={`${formId}-channel`}>Channel</Label>
              <SimpleSelect
                id={`${formId}-channel`}
                value={draft.channelId}
                placeholder="Choose a channel"
                onValueChange={(next) => setDraft({ ...draft, channelId: next })}
                options={channels.map((channel) => ({ value: channel.id, label: channel.name }))}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor={`${formId}-cadence`}>Cadence</Label>
              <SimpleSelect
                id={`${formId}-cadence`}
                value={draft.cadence}
                onValueChange={(next) => setDraft({ ...draft, cadence: next as DigestSpec["cadence"] })}
                options={[
                  { value: "daily", label: "Daily" },
                  { value: "weekly", label: "Weekly" },
                ]}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor={`${formId}-hour`}>Hour (UTC)</Label>
              <Input
                id={`${formId}-hour`}
                type="number"
                min={0}
                max={23}
                value={draft.hourUtc}
                onChange={(e) => setDraft({ ...draft, hourUtc: Number(e.target.value) })}
              />
            </div>
            {draft.cadence === "weekly" && (
              <div className="grid gap-2">
                <Label htmlFor={`${formId}-day`}>Weekday</Label>
                <SimpleSelect
                  id={`${formId}-day`}
                  value={String(draft.weekday)}
                  onValueChange={(next) => setDraft({ ...draft, weekday: Number(next) })}
                  options={WEEKDAYS.map((day, index) => ({ value: String(index), label: day }))}
                />
              </div>
            )}
            <label className="flex items-center gap-2 text-sm md:col-span-2">
              <input
                type="checkbox"
                className="size-4 accent-primary"
                checked={draft.enabled}
                onChange={(e) => setDraft({ ...draft, enabled: e.target.checked })}
              />
              Enabled
            </label>
            <div>
              <Button type="submit" size="sm" disabled={!draft.name.trim() || !draft.channelId}>
                Save digest
              </Button>
            </div>
          </form>
        )}
      </CardContent>
    </Card>
  );
}
