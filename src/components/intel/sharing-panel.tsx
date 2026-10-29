import { useEffect, useId, useState } from "react";
import { useLocation } from "react-router-dom";
import { toast } from "sonner";
import { Notice } from "@/components/status/notice";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { deleteView, downloadFile, exportConfig, importConfig, listViews, upsertView } from "@/pages/intel/api";
import type { SavedView } from "@/pages/intel/types";

export function SharingPanel() {
  const location = useLocation();
  const formId = useId();
  const [views, setViews] = useState<SavedView[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [path, setPath] = useState(location.pathname);
  const [note, setNote] = useState("");

  const load = () => {
    listViews()
      .then((next) => {
        setViews(next);
        setError(null);
      })
      .catch((err) => setError(String(err)));
  };

  useEffect(() => {
    load();
  }, []);

  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Config for teammates</CardTitle>
          <CardDescription>
            JSON includes servers without passwords, route rules, Redis targets, alert rules,
            intelligence rules, channel settings, saved views, and digests. Webhook URLs, tokens, and
            SMTP passwords stay in Credential Manager on this PC.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-wrap gap-2">
          <Button
            type="button"
            onClick={async () => {
              try {
                downloadFile(await exportConfig());
              } catch (err) {
                toast.error(String(err));
              }
            }}
          >
            Export JSON
          </Button>
          <Label className="inline-flex h-8 cursor-pointer items-center rounded-lg border border-border px-2.5 text-sm focus-within:ring-3 focus-within:ring-ring">
            Import JSON
            <input
              type="file"
              accept="application/json,.json"
              className="sr-only"
              onChange={async (event) => {
                const file = event.target.files?.[0];
                event.target.value = "";
                if (!file) return;
                try {
                  const summary = await importConfig(await file.text());
                  toast.success(
                    `Imported ${summary.servers} servers, ${summary.alertRules + summary.intelRules} rules, ${summary.channels} channels`,
                  );
                  load();
                } catch (err) {
                  toast.error(String(err));
                }
              }}
            />
          </Label>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Saved views</CardTitle>
          <CardDescription>A name, an in-app path, and a note you can hand to a teammate in the export.</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          {error && (
            <Notice tone="critical" title="Could not load views">
              {error}
            </Notice>
          )}
          <form
            className="grid gap-3 md:grid-cols-2"
            onSubmit={async (event) => {
              event.preventDefault();
              try {
                await upsertView({ id: "", name: name.trim(), path: path.trim(), note: note.trim() });
                setName("");
                setNote("");
                load();
              } catch (err) {
                toast.error(String(err));
              }
            }}
          >
            <div className="grid gap-2">
              <Label htmlFor={`${formId}-name`}>Name</Label>
              <Input id={`${formId}-name`} value={name} onChange={(e) => setName(e.target.value)} required />
            </div>
            <div className="grid gap-2">
              <Label htmlFor={`${formId}-path`}>Path</Label>
              <Input id={`${formId}-path`} className="font-mono" value={path} onChange={(e) => setPath(e.target.value)} required />
            </div>
            <div className="grid gap-2 md:col-span-2">
              <Label htmlFor={`${formId}-note`}>Note</Label>
              <Input id={`${formId}-note`} value={note} onChange={(e) => setNote(e.target.value)} />
            </div>
            <div>
              <Button type="submit" size="sm" disabled={!name.trim() || !path.startsWith("/")}>
                Save view
              </Button>
            </div>
          </form>
          {views.length === 0 ? (
            <p className="text-sm text-muted-foreground">No saved views yet.</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {views.map((view) => (
                <li key={view.id} className="flex items-start justify-between gap-3 rounded-lg border border-border px-3 py-2">
                  <div>
                    <p className="text-sm font-medium">{view.name}</p>
                    <p className="font-mono text-xs text-muted-foreground">{view.path}</p>
                    {view.note && <p className="text-xs text-muted-foreground">{view.note}</p>}
                  </div>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={async () => {
                      try {
                        await deleteView(view.id);
                        load();
                      } catch (err) {
                        toast.error(String(err));
                      }
                    }}
                  >
                    Delete
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
