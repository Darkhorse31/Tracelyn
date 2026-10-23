import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { AppShell } from "@/components/layout/app-shell";
import { ServersProvider } from "@/contexts/servers-context";
import { OverviewPage } from "@/pages/overview-page";
import { SettingsPage } from "@/pages/settings-page";

function App() {
  return (
    <ServersProvider>
      <BrowserRouter>
        <Routes>
          <Route element={<AppShell />}>
            <Route index element={<OverviewPage />} />
            <Route path="settings" element={<SettingsPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </ServersProvider>
  );
}

export default App;
