import { SidebarTrigger } from "@/components/ui/sidebar";
import { Separator } from "@/components/ui/separator";
import { ThemeMenu } from "@/components/theme/theme-controls";
import { TimeRangePicker } from "@/components/time-range/time-range-picker";
import { PrivacyToggle, usePrivacy } from "@/contexts/privacy-context";
import { useTimeRange } from "@/contexts/time-range-context";

export function TopBar({ title }: { title: string }) {
  const { range, setPreset, setCustomRange } = useTimeRange();
  const { enabled } = usePrivacy();

  return (
    <header className="sticky top-0 z-20 flex h-14 shrink-0 items-center gap-2 border-b border-border bg-background/95 px-4 backdrop-blur supports-backdrop-filter:bg-background/80">
      <SidebarTrigger />
      <Separator orientation="vertical" className="mx-1 h-6" />
      <h1 className="text-sm font-semibold tracking-tight">{title}</h1>
      <div className="ml-auto flex items-center gap-2">
        {enabled && (
          <span className="rounded-md border border-border bg-muted px-2 py-0.5 text-xs font-medium text-foreground">
            Privacy mode
          </span>
        )}
        <PrivacyToggle />
        <TimeRangePicker
          value={range}
          onChange={(next) => {
            if (next.preset === "custom") {
              setCustomRange(next.startMs, next.endMs);
            } else {
              setPreset(next.preset);
            }
          }}
        />
        <Separator orientation="vertical" className="mx-1 h-6" />
        <ThemeMenu />
      </div>
    </header>
  );
}
