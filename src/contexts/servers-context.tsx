import { createContext, useContext } from "react";
import { useServers, type ServersContextValue } from "@/hooks/use-servers";

const ServersContext = createContext<ServersContextValue | null>(null);

export function ServersProvider({ children }: { children: React.ReactNode }) {
  const value = useServers();
  return (
    <ServersContext.Provider value={value}>{children}</ServersContext.Provider>
  );
}

export function useServersContext() {
  const ctx = useContext(ServersContext);
  if (!ctx) {
    throw new Error("useServersContext must be used within ServersProvider");
  }
  return ctx;
}
