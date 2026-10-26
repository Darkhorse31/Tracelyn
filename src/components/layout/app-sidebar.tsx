import { NavLink } from "react-router-dom";
import {
  Activity,
  AlertTriangle,
  BarChart3,
  Boxes,
  Bug,
  Database,
  LayoutDashboard,
  Plus,
  Radar,
  Route,
  ScrollText,
  Settings,
  TerminalSquare,
  ClipboardList,
  Share2,
  Timer,
  Waypoints,
  ShieldCheck,
  HardDrive,
  Network,
  ListOrdered,
  LockKeyhole,
  CircleHelp,
  ListChecks,
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
import {
  CONNECTION_STATUS_LABELS,
  ConnectionStatusBadge,
  ConnectionStatusIcon,
} from "@/components/status/connection-status-badge";
import { useServersContext } from "@/contexts/servers-context";
import { cn } from "cn";
import { useState } from "react";
import markLight from "@/assets/tracelyn-mark.png";
import markDark from "@/assets/tracelyn-mark-dark.png";

const NAV = [
  { to: "/", label: "Overview", icon: LayoutDashboard },
  { to: "/server", label: "Server", icon: Activity },
  { to: "/apps", label: "Applications", icon: Boxes },
  { to: "/logs", label: "Logs", icon: ScrollText },
  { to: "/web", label: "Web traffic", icon: BarChart3 },
  { to: "/apm", label: "API endpoints", icon: Route },
  { to: "/traces", label: "Traces", icon: Waypoints },
  { to: "/map", label: "Service map", icon: Share2 },
  { to: "/errors", label: "Errors", icon: Bug },
  { to: "/slow-queries", label: "Slow queries", icon: Timer },
  { to: "/redis", label: "Redis", icon: Database },
  { to: "/alerts", label: "Alerts", icon: AlertTriangle },
  { to: "/settings", label: "Settings", icon: Settings },
] as const;

const INFRA_NAV = [
  { to: "/infra/databases", label: "Databases", icon: Database },
  { to: "/infra/queues", label: "Queues", icon: ListOrdered },
  { to: "/infra/balancers", label: "Load balancers", icon: Network },
  { to: "/infra/containers", label: "Containers", icon: Boxes },
  { to: "/infra/certificates", label: "Certificates and uptime", icon: LockKeyhole },
  { to: "/infra/cron", label: "Cron", icon: Timer },
  { to: "/infra/capacity", label: "Capacity", icon: HardDrive },
  { to: "/infra/security", label: "Security", icon: ShieldCheck },
] as const;

const OPS_NAV = [
  { to: "/ops", label: "Audit", icon: ClipboardList },
  { to: "/ops/terminal", label: "Terminal", icon: TerminalSquare },
] as const;

/** Inset bar marks the current item with shape as well as color. */
const ACTIVE_ITEM =
  "bg-sidebar-accent font-medium text-sidebar-accent-foreground shadow-[inset_2px_0_0_var(--sidebar-primary)]";

export function AppSidebar() {
  const { servers, statusById, selectedServerId, setSelectedServerId } =
    useServersContext();
  const [addOpen, setAddOpen] = useState(false);
  const selectedStatus = selectedServerId
    ? statusById.get(selectedServerId)
    : undefined;

  return (
    <>
      <Sidebar collapsible="icon">
        <SidebarHeader className="gap-2 p-2">
          <div className="flex h-10 items-center gap-2 px-2">
            <div className="flex size-8 shrink-0 items-center justify-center">
              <img src={markLight} alt="" className="size-6 dark:hidden" />
              <img src={markDark} alt="" className="hidden size-6 dark:block" />
            </div>
            <div className="flex flex-col group-data-[collapsible=icon]:hidden">
              <span className="text-sm font-semibold tracking-tight">Tracelyn</span>
              <span className="text-xs text-muted-foreground">SSH monitoring</span>
            </div>
          </div>
        </SidebarHeader>
        <SidebarContent>
          <nav aria-label="Views">
            <SidebarGroup>
              <SidebarGroupLabel>Views</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  {NAV.map((item) => (
                    <SidebarMenuItem key={item.to}>
                      <SidebarMenuButton
                        render={
                          <NavLink
                            to={item.to}
                            end={item.to === "/"}
                            className={({ isActive }) => cn(isActive && ACTIVE_ITEM)}
                          />
                        }
                        tooltip={item.label}
                      >
                        <item.icon aria-hidden />
                        <span>{item.label}</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          </nav>
          <nav aria-label="Infrastructure">
            <SidebarGroup>
              <SidebarGroupLabel>Infrastructure</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  {INFRA_NAV.map((item) => (
                    <SidebarMenuItem key={item.to}>
                      <SidebarMenuButton render={<NavLink to={item.to} className={({ isActive }) => cn(isActive && ACTIVE_ITEM)} />} tooltip={item.label}>
                        <item.icon aria-hidden /><span>{item.label}</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          </nav>
          <nav aria-label="Operations">
            <SidebarGroup>
              <SidebarGroupLabel>Operations</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  {OPS_NAV.map((item) => (
                    <SidebarMenuItem key={item.to}>
                      <SidebarMenuButton render={<NavLink to={item.to} end={item.to === "/ops"} className={({ isActive }) => cn(isActive && ACTIVE_ITEM)} />} tooltip={item.label}>
                        <item.icon aria-hidden /><span>{item.label}</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          </nav>
          <nav aria-label="Teams and intelligence">
            <SidebarGroup>
              <SidebarGroupLabel>Teams and intelligence</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  <SidebarMenuItem>
                    <SidebarMenuButton render={<NavLink to="/intel" className={({ isActive }) => cn(isActive && ACTIVE_ITEM)} />} tooltip="Intelligence">
                      <Radar aria-hidden /><span>Intelligence</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          </nav>
          <nav aria-label="Help">
            <SidebarGroup>
              <SidebarGroupLabel>Help</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  <SidebarMenuItem>
                    <SidebarMenuButton render={<NavLink to="/setup" className={({ isActive }) => cn(isActive && ACTIVE_ITEM)} />} tooltip="Setup guide">
                      <ListChecks aria-hidden /><span>Setup guide</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                  <SidebarMenuItem>
                    <SidebarMenuButton render={<NavLink to="/how-it-works" className={({ isActive }) => cn(isActive && ACTIVE_ITEM)} />} tooltip="How Tracelyn works">
                      <CircleHelp aria-hidden /><span>How Tracelyn works</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          </nav>
          <SidebarSeparator />
          <SidebarGroup aria-labelledby="sidebar-servers-label">
            <div className="flex items-center justify-between pr-1">
              <SidebarGroupLabel id="sidebar-servers-label">Servers</SidebarGroupLabel>
              <Button
                variant="ghost"
                size="icon-xs"
                onClick={() => setAddOpen(true)}
                aria-label="Add server"
                className="group-data-[collapsible=icon]:hidden"
              >
                <Plus />
              </Button>
            </div>
            <SidebarGroupContent>
              <SidebarMenu>
                {servers.length === 0 ? (
                  <p className="px-2 py-2 text-xs text-muted-foreground group-data-[collapsible=icon]:hidden">
                    No servers yet. Add one to get started.
                  </p>
                ) : (
                  servers.map((server) => {
                    const status = statusById.get(server.id)?.status;
                    const statusLabel = CONNECTION_STATUS_LABELS[status ?? "disconnected"];
                    const selected = selectedServerId === server.id;
                    return (
                      <SidebarMenuItem key={server.id}>
                        <SidebarMenuButton
                          isActive={selected}
                          aria-current={selected ? "true" : undefined}
                          onClick={() => setSelectedServerId(server.id)}
                          tooltip={`${server.name} — ${statusLabel}`}
                          className={cn(selected && ACTIVE_ITEM)}
                        >
                          <ConnectionStatusIcon status={status} />
                          <span className="truncate">{server.name}</span>
                          <span className="sr-only">, {statusLabel}</span>
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
          {selectedServerId && selectedStatus && (
            <div className="flex items-center justify-between gap-2 rounded-lg border border-sidebar-border bg-sidebar-accent/40 px-3 py-2">
              <span className="text-xs text-muted-foreground">Selected server</span>
              <ConnectionStatusBadge status={selectedStatus.status} />
            </div>
          )}
        </SidebarFooter>
        <SidebarRail />
      </Sidebar>
      <ServerFormDialog open={addOpen} onOpenChange={setAddOpen} />
    </>
  );
}
