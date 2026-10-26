import { useId, useState } from "react";
import { Clock, Pencil } from "lucide-react";
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
import {
  resolveTimeRange,
  TIME_RANGE_LABELS,
  type TimeRangePreset,
  type TimeRangeSelection,
} from "@/lib/time-range";
import { cn } from "cn";

export interface TimeRangePickerProps {
  value: TimeRangeSelection;
  onChange: (next: TimeRangeSelection) => void;
  disabled?: boolean;
  className?: string;
}

const PRESETS: TimeRangePreset[] = ["live", "15m", "1h", "24h", "7d", "custom"];

/** `datetime-local` wants local wall-clock time without a zone suffix. */
function toLocalInput(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function formatRange(startMs: number, endMs: number): string {
  const fmt = new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
  return `${fmt.format(startMs)} – ${fmt.format(endMs)}`;
}

function CustomRangeDialog({
  open,
  onOpenChange,
  initial,
  onApply,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initial: { startMs: number; endMs: number };
  onApply: (startMs: number, endMs: number) => void;
}) {
  const id = useId();
  const [start, setStart] = useState(() => toLocalInput(initial.startMs));
  const [end, setEnd] = useState(() => toLocalInput(initial.endMs));

  const startMs = new Date(start).getTime();
  const endMs = new Date(end).getTime();
  const error =
    Number.isNaN(startMs) || Number.isNaN(endMs)
      ? "Enter both a start and an end time."
      : startMs >= endMs
        ? "Start must be before end."
        : endMs > Date.now() + 60_000
          ? "End cannot be in the future."
          : null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form
          className="grid gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (error) return;
            onApply(startMs, endMs);
            onOpenChange(false);
          }}
        >
          <DialogHeader>
            <DialogTitle>Custom time range</DialogTitle>
            <DialogDescription>
              Historical charts query stored samples for this window. Times are local.
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="grid gap-2">
              <Label htmlFor={`${id}-start`}>Start</Label>
              <Input
                id={`${id}-start`}
                type="datetime-local"
                value={start}
                max={end}
                onChange={(e) => setStart(e.target.value)}
                aria-invalid={error ? true : undefined}
                aria-describedby={error ? `${id}-error` : undefined}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor={`${id}-end`}>End</Label>
              <Input
                id={`${id}-end`}
                type="datetime-local"
                value={end}
                min={start}
                onChange={(e) => setEnd(e.target.value)}
                aria-invalid={error ? true : undefined}
                aria-describedby={error ? `${id}-error` : undefined}
              />
            </div>
          </div>
          <p
            id={`${id}-error`}
            className="min-h-5 text-sm text-status-critical"
            role="alert"
          >
            {error}
          </p>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={Boolean(error)}>
              Apply
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function TimeRangePicker({
  value,
  onChange,
  disabled,
  className,
}: TimeRangePickerProps) {
  const [customOpen, setCustomOpen] = useState(false);
  const isCustom = value.preset === "custom";

  const handlePreset = (preset: TimeRangePreset) => {
    if (preset === "custom") {
      setCustomOpen(true);
      return;
    }
    onChange(resolveTimeRange(preset, Date.now()));
  };

  return (
    <div className={cn("flex items-center gap-2", className)}>
      {isCustom && (
        <span className="hidden font-mono text-xs text-muted-foreground lg:inline">
          {formatRange(value.startMs, value.endMs)}
        </span>
      )}
      <Select
        value={value.preset}
        onValueChange={(v) => handlePreset(v as TimeRangePreset)}
        disabled={disabled}
      >
        <SelectTrigger className="w-44" aria-label="Time range">
          <Clock data-icon="inline-start" aria-hidden />
          <SelectValue>
            {(v: TimeRangePreset) => TIME_RANGE_LABELS[v] ?? "Time range"}
          </SelectValue>
        </SelectTrigger>
        <SelectContent>
          {PRESETS.map((p) => (
            <SelectItem key={p} value={p} className="min-h-8">
              {p === "custom" ? "Custom range…" : TIME_RANGE_LABELS[p]}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {isCustom && (
        <Button
          variant="ghost"
          size="icon"
          onClick={() => setCustomOpen(true)}
          aria-label={`Edit custom range, ${formatRange(value.startMs, value.endMs)}`}
          disabled={disabled}
        >
          <Pencil />
        </Button>
      )}
      {customOpen && (
        <CustomRangeDialog
          open={customOpen}
          onOpenChange={setCustomOpen}
          initial={
            isCustom
              ? value
              : { startMs: Date.now() - 60 * 60 * 1000, endMs: Date.now() }
          }
          onApply={(startMs, endMs) =>
            onChange(resolveTimeRange("custom", Date.now(), { startMs, endMs }))
          }
        />
      )}
    </div>
  );
}
