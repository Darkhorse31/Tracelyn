import { Monitor, Moon, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  THEME_OPTIONS,
  type ThemePreference,
  useTheme,
} from "@/contexts/theme-context";
import { cn } from "cn";

const ICONS: Record<ThemePreference, typeof Sun> = {
  light: Sun,
  dark: Moon,
  system: Monitor,
};

const LABELS = Object.fromEntries(
  THEME_OPTIONS.map((o) => [o.value, o.label]),
) as Record<ThemePreference, string>;

/** Compact theme switcher for the top bar. */
export function ThemeMenu() {
  const { theme, resolvedTheme, setTheme } = useTheme();
  const Icon = theme === "system" ? Monitor : resolvedTheme === "dark" ? Moon : Sun;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon"
            aria-label={`Theme: ${LABELS[theme]}. Change theme`}
          />
        }
      >
        <Icon />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-40">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Theme</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={theme}
            onValueChange={(value) => setTheme(value as ThemePreference)}
          >
            {THEME_OPTIONS.map(({ value, label }) => {
              const ItemIcon = ICONS[value];
              return (
                <DropdownMenuRadioItem key={value} value={value} className="min-h-8">
                  <ItemIcon aria-hidden />
                  {label}
                </DropdownMenuRadioItem>
              );
            })}
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Segmented control for Settings; native radios give arrow-key navigation. */
export function ThemeRadioGroup({ className }: { className?: string }) {
  const { theme, resolvedTheme, setTheme } = useTheme();

  return (
    <fieldset className={cn("flex flex-col gap-2", className)}>
      <legend className="sr-only">Theme</legend>
      <div className="inline-flex w-fit rounded-lg border border-input p-0.5">
        {THEME_OPTIONS.map(({ value, label }) => {
          const ItemIcon = ICONS[value];
          return (
            <label
              key={value}
              className={cn(
                "relative flex h-8 min-w-24 cursor-pointer items-center justify-center gap-2 rounded-md px-3 text-sm font-medium transition-colors duration-150 has-focus-visible:ring-3 has-focus-visible:ring-ring",
                theme === value
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )}
            >
              <input
                type="radio"
                name="theme"
                value={value}
                checked={theme === value}
                onChange={() => setTheme(value)}
                className="sr-only"
              />
              <ItemIcon className="size-4" aria-hidden />
              {label}
            </label>
          );
        })}
      </div>
      {theme === "system" && (
        <p className="text-xs text-muted-foreground">
          Following your operating system — currently {resolvedTheme}.
        </p>
      )}
    </fieldset>
  );
}
