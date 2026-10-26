import { SidebarTrigger } from "@/components/ui/sidebar";
import { Separator } from "@/components/ui/separator";
import { ThemeMenu } from "@/components/theme/theme-controls";
import { TimeRangePicker } from "@/components/time-range/time-range-picker";
import { useTimeRange } from "@/contexts/time-range-context";

export function TopBar({ title }: { title: string }) {
  const { range, setPreset, setCustomRange } = useTimeRange();

  return (
    <header className="sticky top-0 z-20 flex h-14 shrink-0 items-center gap-2 border-b border-border bg-background/95 px-4 backdrop-blur supports-backdrop-filter:bg-background/80">
      <SidebarTrigger />
      <Separator orientation="vertical" className="mx-1 h-6" />
      <h1 className="text-sm font-semibold tracking-tight">{title}</h1>
      <div className="ml-auto flex items-center gap-2">
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
