"use client"

import { Toaster as Sonner, type ToasterProps } from "sonner"
import { CircleCheckIcon, InfoIcon, TriangleAlertIcon, OctagonXIcon, Loader2Icon } from "lucide-react"
import { useTheme } from "@/contexts/theme-context"

const Toaster = ({ ...props }: ToasterProps) => {
  const { resolvedTheme } = useTheme()

  return (
    <Sonner
      theme={resolvedTheme}
      className="toaster group"
      icons={{
        success: (
          <CircleCheckIcon className="size-4" />
        ),
        info: (
          <InfoIcon className="size-4" />
        ),
        warning: (
          <TriangleAlertIcon className="size-4" />
        ),
        error: (
          <OctagonXIcon className="size-4" />
        ),
        loading: (
          <Loader2Icon className="size-4 animate-spin" />
        ),
      }}
      style={
        {
          "--normal-bg": "var(--popover)",
          "--normal-text": "var(--popover-foreground)",
          "--normal-border": "var(--border)",
          "--success-bg": "color-mix(in oklch, var(--status-ok) 12%, var(--popover))",
          "--success-text": "var(--status-ok)",
          "--success-border": "color-mix(in oklch, var(--status-ok) 40%, var(--popover))",
          "--info-bg": "color-mix(in oklch, var(--status-info) 12%, var(--popover))",
          "--info-text": "var(--status-info)",
          "--info-border": "color-mix(in oklch, var(--status-info) 40%, var(--popover))",
          "--warning-bg": "color-mix(in oklch, var(--status-warning) 12%, var(--popover))",
          "--warning-text": "var(--status-warning)",
          "--warning-border": "color-mix(in oklch, var(--status-warning) 40%, var(--popover))",
          "--error-bg": "color-mix(in oklch, var(--status-critical) 12%, var(--popover))",
          "--error-text": "var(--status-critical)",
          "--error-border": "color-mix(in oklch, var(--status-critical) 40%, var(--popover))",
          "--border-radius": "var(--radius)",
        } as React.CSSProperties
      }
      toastOptions={{
        classNames: {
          toast: "cn-toast",
        },
      }}
      {...props}
    />
  )
}

export { Toaster }
