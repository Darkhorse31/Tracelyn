import React from "react";
import ReactDOM from "react-dom/client";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applyTheme, readStoredTheme, ThemeProvider } from "@/contexts/theme-context";
import { ErrorBoundary } from "@/components/status/error-boundary";
import App from "./App";
import "./index.css";

applyTheme(readStoredTheme());

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider>
      <TooltipProvider>
        <ErrorBoundary title="Tracelyn hit an unexpected error">
          <App />
        </ErrorBoundary>
        <Toaster richColors position="bottom-right" />
      </TooltipProvider>
    </ThemeProvider>
  </React.StrictMode>,
);
