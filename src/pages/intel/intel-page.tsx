import { useSearchParams } from "react-router-dom";
import { AnomalyPanel } from "@/components/intel/anomaly-panel";
import { ChannelsPanel } from "@/components/intel/channels-panel";
import { DigestPanel } from "@/components/intel/digest-panel";
import { IncidentsPanel } from "@/components/intel/incidents-panel";
import { RulesPanel } from "@/components/intel/rules-panel";
import { SharingPanel } from "@/components/intel/sharing-panel";
import { cn } from "@/lib/utils";

const TABS = [
  ["incidents", "Incidents"],
  ["rules", "Rules"],
  ["channels", "Channels"],
  ["anomalies", "Anomalies"],
  ["digest", "Digest"],
  ["sharing", "Sharing"],
] as const;

type TabId = (typeof TABS)[number][0];

function isTab(value: string | null): value is TabId {
  return TABS.some(([id]) => id === value);
}

export function IntelPage() {
  const [params, setParams] = useSearchParams();
  const requested = params.get("tab");
  const tab: TabId = isTab(requested) ? requested : "incidents";

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h2 className="text-lg font-semibold tracking-tight">Intelligence</h2>
        <p className="text-sm text-muted-foreground">
          Rules, delivery channels, anomaly bands, incidents, and a digest you can share with a teammate.
        </p>
      </div>
      <div role="tablist" aria-label="Intelligence sections" className="flex flex-wrap gap-1">
        {TABS.map(([id, label]) => {
          const selected = tab === id;
          return (
            <button
              key={id}
              type="button"
              role="tab"
              id={`intel-tab-${id}`}
              aria-selected={selected}
              aria-controls="intel-panel"
              tabIndex={selected ? 0 : -1}
              className={cn(
                "h-8 rounded-lg px-3 text-sm focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring",
                selected ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-muted",
              )}
              onClick={() => setParams(id === "incidents" ? {} : { tab: id }, { replace: true })}
              onKeyDown={(event) => {
                const index = TABS.findIndex(([value]) => value === id);
                const next = event.key === "ArrowRight" ? (index + 1) % TABS.length
                  : event.key === "ArrowLeft" ? (index + TABS.length - 1) % TABS.length
                  : event.key === "Home" ? 0
                  : event.key === "End" ? TABS.length - 1
                  : null;
                if (next === null) return;
                event.preventDefault();
                const nextId = TABS[next][0];
                setParams(nextId === "incidents" ? {} : { tab: nextId }, { replace: true });
                document.getElementById(`intel-tab-${nextId}`)?.focus();
              }}
            >
              {label}
            </button>
          );
        })}
      </div>
      <div role="tabpanel" id="intel-panel" tabIndex={0} aria-labelledby={`intel-tab-${tab}`}>
        {tab === "incidents" && <IncidentsPanel />}
        {tab === "rules" && <RulesPanel />}
        {tab === "channels" && <ChannelsPanel />}
        {tab === "anomalies" && <AnomalyPanel />}
        {tab === "digest" && <DigestPanel />}
        {tab === "sharing" && <SharingPanel />}
      </div>
    </div>
  );
}
