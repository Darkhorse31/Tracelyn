import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { AppShell } from "@/components/layout/app-shell";
import { ServersProvider } from "@/contexts/servers-context";
import { OverviewPage } from "@/pages/overview-page";
import { ApmEndpointsPage } from "@/pages/apm-endpoints-page";
import { RedisPage } from "@/pages/redis-page";
import { ServerPage } from "@/pages/server-page";
import { SettingsPage } from "@/pages/settings-page";
import { WebTrafficPage } from "@/pages/web-traffic-page";

function App() {
  return (
    <ServersProvider>
      <BrowserRouter>
        <Routes>
          <Route element={<AppShell />}>
            <Route index element={<OverviewPage />} />
            <Route path="server" element={<ServerPage />} />
            <Route path="web" element={<WebTrafficPage />} />
            <Route path="apm" element={<ApmEndpointsPage />} />
            <Route path="redis" element={<RedisPage />} />
            <Route path="settings" element={<SettingsPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </ServersProvider>
  );
}

export default App;
