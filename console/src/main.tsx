import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { createAdminApi } from "./api";
import "./styles.css";

// Same-origin admin API; a 401 sends the browser to GitHub login (spec §3.3).
const api = createAdminApi();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App api={api} />
  </StrictMode>,
);
