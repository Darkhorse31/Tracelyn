import { Outlet, useLocation } from "react-router-dom";
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
  "/settings": "Settings",
};

export function AppShell() {
  const location = useLocation();
  const title = TITLES[location.pathname] ?? "Tracelyn";

  return (
    <TimeRangeProvider>
      <SidebarProvider defaultOpen>
        <AppSidebar />
        <SidebarInset className="min-h-svh">
          <TopBar title={title} />
          <main className="flex flex-1 flex-col gap-4 p-4 md:p-6">
            <Outlet />
          </main>
        </SidebarInset>
      </SidebarProvider>
    </TimeRangeProvider>
  );
}
