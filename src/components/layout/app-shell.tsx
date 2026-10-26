import { useEffect } from "react";
import { Outlet, useLocation } from "react-router-dom";
import { AlertAnnouncer } from "@/components/alerts/alert-announcer";
import { AppSidebar } from "@/components/layout/app-sidebar";
import { TopBar } from "@/components/layout/top-bar";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";
import { TimeRangeProvider } from "@/contexts/time-range-context";

const TITLES: Record<string, string> = {
  "/": "Overview",
  "/server": "Server",
  "/web": "Web traffic",
  "/apm": "API endpoints",
  "/redis": "Redis",
  "/alerts": "Alerts",
  "/ops": "Audit",
  "/ops/terminal": "Terminal",
  "/settings": "Settings",
};

export function AppShell() {
  const location = useLocation();
  const title = TITLES[location.pathname] ?? "Tracelyn";

  useEffect(() => {
    document.title = title === "Tracelyn" ? title : `${title} · Tracelyn`;
  }, [title]);

  return (
    <TimeRangeProvider>
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:rounded-lg focus:bg-primary focus:px-3 focus:py-2 focus:text-sm focus:font-medium focus:text-primary-foreground"
      >
        Skip to content
      </a>
      <SidebarProvider defaultOpen>
        <AppSidebar />
        <SidebarInset className="min-h-svh min-w-0">
          <TopBar title={title} />
          <main
            id="main"
            tabIndex={-1}
            className="flex flex-1 flex-col gap-6 p-6 outline-none"
          >
            <Outlet />
          </main>
        </SidebarInset>
      </SidebarProvider>
      <AlertAnnouncer />
    </TimeRangeProvider>
  );
}
