import { NavLink } from "react-router-dom";
import {
  Activity,
  AlertTriangle,
  BarChart3,
  Database,
  LayoutDashboard,
  Plus,
  Route,
  Settings,
} from "lucide-react";
import { ServerFormDialog } from "@/components/servers/server-form-dialog";
import { Button } from "@/components/ui/button";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
  SidebarSeparator,
} from "@/components/ui/sidebar";
import { ConnectionStatusBadge } from "@/components/status/connection-status-badge";
import { useServersContext } from "@/contexts/servers-context";
import { cn } from "cn";
import { useState } from "react";

const NAV = [
  { to: "/", label: "Overview", icon: LayoutDashboard, enabled: true },
  { to: "/server", label: "Server", icon: Activity, enabled: true },
  { to: "/web", label: "Web traffic", icon: BarChart3, enabled: true },
  { to: "/apm", label: "API endpoints", icon: Route, enabled: true },
  { to: "/redis", label: "Redis", icon: Database, enabled: false },
  { to: "/alerts", label: "Alerts", icon: AlertTriangle, enabled: false },
  { to: "/settings", label: "Settings", icon: Settings, enabled: true },
] as const;

export function AppSidebar() {
  const { servers, statusById, selectedServerId, setSelectedServerId } =
    useServersContext();
  const [addOpen, setAddOpen] = useState(false);

  return (
    <>
      <Sidebar collapsible="icon">
        <SidebarHeader className="gap-2 p-2">
          <div className="flex items-center gap-2 px-2 py-1">
            <div className="flex size-8 items-center justify-center rounded-lg bg-primary text-primary-foreground">
              <Activity />
            </div>
            <div className="flex flex-col group-data-[collapsible=icon]:hidden">
              <span className="text-sm font-semibold tracking-tight">Tracelyn</span>
              <span className="text-xs text-muted-foreground">SSH monitoring</span>
            </div>
          </div>
        </SidebarHeader>
        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupLabel>Views</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {NAV.map((item) => (
                  <SidebarMenuItem key={item.to}>
                    <SidebarMenuButton
                      render={
                        item.enabled
                          ? (
                              <NavLink
                                to={item.to}
                                className={({ isActive }) =>
                                  cn(isActive && "bg-sidebar-accent")
                                }
                              />
                            )
                          : (
                              <span
                                className="pointer-events-none opacity-50"
                                aria-disabled
                              />
                            )
                      }
                      tooltip={item.enabled ? item.label : `${item.label} (soon)`}
                    >
                      <item.icon />
                      <span>{item.label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
          <SidebarSeparator />
          <SidebarGroup>
            <div className="flex items-center justify-between px-2">
              <SidebarGroupLabel>Servers</SidebarGroupLabel>
              <Button
                variant="ghost"
                size="icon-xs"
                onClick={() => setAddOpen(true)}
                aria-label="Add server"
              >
                <Plus />
              </Button>
            </div>
            <SidebarGroupContent>
              <SidebarMenu>
                {servers.length === 0 ? (
                  <p className="px-3 py-2 text-xs text-muted-foreground group-data-[collapsible=icon]:hidden">
                    No servers yet. Add one to get started.
                  </p>
                ) : (
                  servers.map((server) => {
                    const status = statusById.get(server.id);
                    return (
                      <SidebarMenuItem key={server.id}>
                        <SidebarMenuButton
                          isActive={selectedServerId === server.id}
                          onClick={() => setSelectedServerId(server.id)}
                          tooltip={server.name}
                        >
                          <span
                            className={cn(
                              "size-2 shrink-0 rounded-full",
                              status?.status === "connected" && "bg-status-ok",
                              status?.status === "connecting" && "bg-status-info",
                              status?.status === "offline" && "bg-status-critical",
                              status?.status === "degraded" && "bg-status-warning",
                              (!status || status.status === "disconnected") &&
                                "bg-muted-foreground/40",
                            )}
                          />
                          <span className="truncate">{server.name}</span>
                        </SidebarMenuButton>
                      </SidebarMenuItem>
                    );
                  })
                )}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>
        <SidebarFooter className="p-2 group-data-[collapsible=icon]:hidden">
          {selectedServerId && statusById.get(selectedServerId) && (
            <div className="rounded-lg border border-sidebar-border bg-sidebar-accent/40 p-2">
              <p className="mb-1 text-xs text-muted-foreground">Selected</p>
              <ConnectionStatusBadge
                status={statusById.get(selectedServerId)!.status}
              />
            </div>
          )}
        </SidebarFooter>
        <SidebarRail />
      </Sidebar>
      <ServerFormDialog open={addOpen} onOpenChange={setAddOpen} />
    </>
  );
}
