import { createContext, useContext, type ReactNode } from "react";
import type { AdminApi } from "./api";

const ApiContext = createContext<AdminApi | null>(null);

export function ApiProvider({ api, children }: { api: AdminApi; children: ReactNode }) {
  return <ApiContext.Provider value={api}>{children}</ApiContext.Provider>;
}

export function useApi(): AdminApi {
  const api = useContext(ApiContext);
  if (!api) throw new Error("useApi must be used inside <ApiProvider>");
  return api;
}
