import { BrowserRouter, Link, Route, Routes } from "react-router";
import { ApiProvider, useApi } from "./ApiContext";
import type { AdminApi } from "./api";
import { LOGIN_PATH } from "./api";
import { Queue } from "./pages/Queue";
import { TicketDetail } from "./pages/TicketDetail";

function Shell() {
  const api = useApi();
  const logout = async () => {
    try {
      await api.logout();
    } finally {
      window.location.assign(LOGIN_PATH);
    }
  };
  return (
    <>
      <nav className="top">
        <Link to="/" className="brand">
          Intake queue
        </Link>
        <button type="button" className="link" onClick={() => void logout()}>
          Log out
        </button>
      </nav>
      <Routes>
        <Route path="/" element={<Queue />} />
        <Route path="/t/:id" element={<TicketDetail />} />
        <Route
          path="*"
          element={
            <main>
              <p>Not found.</p>
              <Link to="/">Back to the queue</Link>
            </main>
          }
        />
      </Routes>
    </>
  );
}

export function App({ api }: { api: AdminApi }) {
  return (
    <ApiProvider api={api}>
      <BrowserRouter basename="/console">
        <Shell />
      </BrowserRouter>
    </ApiProvider>
  );
}
