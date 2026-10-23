import { Clock } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  resolveTimeRange,
  TIME_RANGE_LABELS,
  type TimeRangePreset,
  type TimeRangeSelection,
} from "@/lib/time-range";

export interface TimeRangePickerProps {
  value: TimeRangeSelection;
  onChange: (next: TimeRangeSelection) => void;
  disabled?: boolean;
  className?: string;
}

const PRESETS: TimeRangePreset[] = ["live", "15m", "1h", "24h", "7d"];

export function TimeRangePicker({
  value,
  onChange,
  disabled,
  className,
}: TimeRangePickerProps) {
  const handlePreset = (preset: TimeRangePreset) => {
    if (preset === "custom") return;
    onChange(resolveTimeRange(preset, Date.now()));
  };

  return (
    <div className={className ?? "flex items-center gap-2"}>
      <Select
        value={value.preset === "custom" ? "custom" : value.preset}
        onValueChange={(v) => handlePreset(v as TimeRangePreset)}
        disabled={disabled}
      >
        <SelectTrigger className="w-[160px]">
          <Clock data-icon="inline-start" />
          <SelectValue placeholder="Time range" />
        </SelectTrigger>
        <SelectContent>
          {PRESETS.map((p) => (
            <SelectItem key={p} value={p}>
              {TIME_RANGE_LABELS[p]}
            </SelectItem>
          ))}
          <SelectItem value="custom">{TIME_RANGE_LABELS.custom}</SelectItem>
        </SelectContent>
      </Select>
      <Button
        variant="outline"
        size="sm"
        disabled={disabled}
        onClick={() =>
          onChange({
            preset: "custom",
            startMs: value.startMs,
            endMs: value.endMs,
          })
        }
      >
        Custom range
      </Button>
    </div>
  );
}
