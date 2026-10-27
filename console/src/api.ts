// Admin API client (admin routes; see docs/architecture.md §3). Same-origin: the console
// is served by intake-server under /console and authenticates with the
// session cookie set by GitHub OAuth. A 401 means "go log in".
import type { QueueFilters, QueueResponse, Ticket, TicketDetail, TriageFields } from "./types";

export class ApiError extends Error {
  readonly status: number;
  constructor(status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

export interface AdminApi {
  listTickets(filters: QueueFilters): Promise<QueueResponse>;
  getTicket(id: number): Promise<TicketDetail>;
  approve(id: number, fields: TriageFields): Promise<Ticket>;
  merge(id: number, into: number): Promise<Ticket>;
  dismiss(id: number, reason: string, note?: string): Promise<Ticket>;
  logout(): Promise<void>;
}

export interface AdminApiOptions {
  fetchFn?: typeof fetch;
  baseUrl?: string;
  onUnauthorized?: () => void;
}

export const LOGIN_PATH = "/admin/login";

export function createAdminApi(opts: AdminApiOptions = {}): AdminApi {
  const fetchFn = opts.fetchFn ?? ((input, init) => fetch(input, init));
  const base = (opts.baseUrl ?? "").replace(/\/+$/, "");
  const onUnauthorized = opts.onUnauthorized ?? (() => window.location.assign(LOGIN_PATH));

  async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const res = await fetchFn(`${base}${path}`, {
      ...init,
      credentials: "same-origin",
      headers: { accept: "application/json", ...(init.body ? { "content-type": "application/json" } : {}), ...init.headers },
    });
    if (res.status === 401) {
      onUnauthorized();
      throw new ApiError(401, "unauthorized");
    }
    if (!res.ok) {
      let message = `request failed with ${res.status}`;
      try {
        const body = (await res.json()) as { error?: string };
        if (body.error) message = body.error;
      } catch {
        // keep the generic message
      }
      throw new ApiError(res.status, message);
    }
    if (res.status === 204) return undefined as T;
    return (await res.json()) as T;
  }

  const post = <T>(path: string, body: unknown) => request<T>(path, { method: "POST", body: JSON.stringify(body) });

  return {
    listTickets(filters) {
      const params = new URLSearchParams();
      for (const [k, v] of Object.entries(filters)) if (v) params.set(k, v);
      const qs = params.toString();
      return request<QueueResponse>(`/admin/tickets${qs ? `?${qs}` : ""}`);
    },
    getTicket: (id) => request<TicketDetail>(`/admin/tickets/${id}`),
    approve: async (id, fields) => (await post<{ ticket: Ticket }>(`/admin/tickets/${id}/approve`, { fields })).ticket,
    merge: async (id, into) => (await post<{ ticket: Ticket }>(`/admin/tickets/${id}/merge`, { into })).ticket,
    dismiss: async (id, reason, note) =>
      (await post<{ ticket: Ticket }>(`/admin/tickets/${id}/dismiss`, note ? { reason, note } : { reason })).ticket,
    logout: () => request<void>("/admin/logout", { method: "POST" }),
  };
}
