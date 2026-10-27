import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { AppShell } from "@/components/layout/app-shell";
import { InfraPage, SECTIONS, type Section } from "@/pages/infra/infra-page";
import { PrivacyProvider } from "@/contexts/privacy-context";
import { ServersProvider } from "@/contexts/servers-context";
import { AppDetailPage } from "@/pages/app-detail-page";
import { AppsPage } from "@/pages/apps-page";
import { ErrorsPage } from "@/pages/errors-page";
import { IncidentPage } from "@/pages/intel/incident-page";
import { IntelPage } from "@/pages/intel/intel-page";
import { LogsPage } from "@/pages/logs-page";
import { OverviewPage } from "@/pages/overview-page";
import { AlertsPage } from "@/pages/alerts-page";
import { ApmEndpointsPage } from "@/pages/apm-endpoints-page";
import { RedisPage } from "@/pages/redis-page";
import { ServerPage } from "@/pages/server-page";
import { ServiceMapPage } from "@/pages/service-map-page";
import { AuditPage } from "@/pages/ops/audit-page";
import { TerminalPage } from "@/pages/ops/terminal-page";
import { SettingsPage } from "@/pages/settings-page";
import { SetupPage } from "@/pages/setup-page";
import { HowItWorksPage } from "@/pages/how-it-works-page";
import { SlowQueriesPage } from "@/pages/slow-queries-page";
import { TraceDetailPage } from "@/pages/trace-detail-page";
import { TracesPage } from "@/pages/traces-page";
import { WebTrafficPage } from "@/pages/web-traffic-page";
import { AllServersPage } from "@/pages/all-servers-page";
import { WelcomePage } from "@/pages/welcome-page";
import { useServersContext } from "@/contexts/servers-context";

function HomePage() {
  const { servers } = useServersContext();
  return servers.length >= 2 ? <AllServersPage /> : <OverviewPage />;
}

function App() {
  return (
    <ServersProvider>
      <PrivacyProvider>
      <BrowserRouter>
        <Routes>
          <Route element={<AppShell />}>
            <Route index element={<HomePage />} />
            <Route path="servers" element={<AllServersPage />} />
            <Route path="welcome" element={<WelcomePage />} />
            <Route path="server" element={<ServerPage />} />
            <Route path="apps" element={<AppsPage />} />
            <Route path="apps/:appId" element={<AppDetailPage />} />
            <Route path="logs" element={<LogsPage />} />
            <Route path="web" element={<WebTrafficPage />} />
            <Route path="apm" element={<ApmEndpointsPage />} />
            <Route path="traces" element={<TracesPage />} />
            <Route path="traces/:traceId" element={<TraceDetailPage />} />
            <Route path="map" element={<ServiceMapPage />} />
            <Route path="errors" element={<ErrorsPage />} />
            <Route path="slow-queries" element={<SlowQueriesPage />} />
            <Route path="redis" element={<RedisPage />} />
            {(Object.keys(SECTIONS) as Section[]).map((section) => (
              <Route key={section} path={`infra/${section}`} element={<InfraPage key={section} section={section} />} />
            ))}
            <Route path="alerts" element={<AlertsPage />} />
            <Route path="intel" element={<IntelPage />} />
            <Route path="intel/incidents/:incidentId" element={<IncidentPage />} />
            <Route path="ops" element={<AuditPage />} />
            <Route path="ops/terminal" element={<TerminalPage />} />
            <Route path="settings" element={<SettingsPage />} />
            <Route path="setup" element={<SetupPage />} />
            <Route path="how-it-works" element={<HowItWorksPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Route>
        </Routes>
      </BrowserRouter>
      </PrivacyProvider>
    </ServersProvider>
  );
}

export default App;
