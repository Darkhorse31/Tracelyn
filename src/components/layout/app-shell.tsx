import { Outlet, useLocation } from "react-router-dom";
import { AppSidebar } from "@/components/layout/app-sidebar";
import { TopBar } from "@/components/layout/top-bar";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";

const TITLES: Record<string, string> = {
  "/": "Overview",
  "/settings": "Settings",
};

export function AppShell() {
  const location = useLocation();
  const title = TITLES[location.pathname] ?? "Tracelyn";

  return (
    <SidebarProvider defaultOpen>
      <AppSidebar />
      <SidebarInset className="min-h-svh">
        <TopBar title={title} />
        <main className="flex flex-1 flex-col gap-4 p-4 md:p-6">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}
