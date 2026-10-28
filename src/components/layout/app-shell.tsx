import { LicenseBanner } from "@/components/account/account-license";
import { useEffect } from "react";
import { Navigate, Outlet, useLocation } from "react-router-dom";
import { AlertAnnouncer } from "@/components/alerts/alert-announcer";
import { ErrorBoundary } from "@/components/status/error-boundary";
import { HostKeyDialog } from "@/components/security/host-key-dialog";
import { AppSidebar } from "@/components/layout/app-sidebar";
import { TopBar } from "@/components/layout/top-bar";
import { PageSkeleton } from "@/components/status/page-skeleton";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";
import { TimeRangeProvider } from "@/contexts/time-range-context";
import { useServersContext } from "@/contexts/servers-context";
import { readPreference } from "@/hooks/use-remembered-state";

const TITLES: Record<string, string> = {
  "/": "Overview",
  "/servers": "All servers",
  "/welcome": "Setup wizard",
  "/apps": "Applications",
  "/logs": "Logs",
  "/traces": "Traces",
  "/setup": "Setup Guide",
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
  const { servers, loading, error } = useServersContext();
  const title =
    location.pathname === "/" && servers.length >= 2
      ? "All servers"
      : (TITLES[location.pathname] ?? "Tracelyn");
  const needsWelcome =
    !loading &&
    !error &&
    servers.length === 0 &&
    !readPreference("onboarding-dismissed", false) &&
    location.pathname !== "/welcome";

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
            <LicenseBanner />
            {loading ? (
              <PageSkeleton />
            ) : needsWelcome ? (
              <Navigate to="/welcome" replace />
            ) : (
              <ErrorBoundary key={location.pathname}>
                <Outlet />
              </ErrorBoundary>
            )}
          </main>
        </SidebarInset>
      </SidebarProvider>
      <AlertAnnouncer />
      <HostKeyDialog />
    </TimeRangeProvider>
  );
}
