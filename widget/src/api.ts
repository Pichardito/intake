// HTTP client for the public conversation API (see docs/architecture.md §3).
// Everything that crosses this boundary has already been scrubbed.

export type ConversationState = "open" | "clarifying" | "closing" | "triaged";

export interface StartRequest {
  site_key: string;
  page_url?: string;
  ua?: string;
  viewport?: string;
}

export interface StartResponse {
  id: string;
  token: string;
}

export interface MessageRequest {
  text: string;
  source: "text";
}

export interface MessageResponse {
  reply: string;
  state: ConversationState;
  /** Server-side redaction counts per label, e.g. `{ SSN: 1 }`. */
  redactions?: Record<string, number>;
}

export interface EndResponse {
  ticket_id: string | null;
  state: ConversationState;
}

export class ApiError extends Error {
  readonly status: number;
  constructor(status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

export interface IntakeApi {
  start(body: StartRequest): Promise<StartResponse>;
  message(id: string, token: string, body: MessageRequest): Promise<MessageResponse>;
  end(id: string, token: string): Promise<EndResponse>;
}

export function createApi(baseUrl: string, fetchFn: typeof fetch = fetch): IntakeApi {
  const base = baseUrl.replace(/\/+$/, "");

  async function post<T>(path: string, body: unknown, token?: string): Promise<T> {
    const headers: Record<string, string> = { "content-type": "application/json" };
    if (token) headers["authorization"] = `Bearer ${token}`;
    const res = await fetchFn(`${base}${path}`, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
      credentials: "omit",
    });
    if (!res.ok) {
      throw new ApiError(res.status, `request failed with ${res.status}`);
    }
    return (await res.json()) as T;
  }

  return {
    start: (body) => post<StartResponse>("/v1/conversations", body),
    message: (id, token, body) =>
      post<MessageResponse>(`/v1/conversations/${encodeURIComponent(id)}/messages`, body, token),
    end: (id, token) => post<EndResponse>(`/v1/conversations/${encodeURIComponent(id)}/end`, {}, token),
  };
}
