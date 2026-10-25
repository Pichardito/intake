import { vi } from "vitest";
import type { AdminApi } from "../src/api";
import type { Ticket, TicketDetail } from "../src/types";

export const sites = [
  { id: 1, name: "Morning Queue site" },
  { id: 2, name: "Morning Queue app" },
];

export function ticket(overrides: Partial<Ticket> = {}): Ticket {
  return {
    id: 1,
    site_id: 1,
    site: "Morning Queue site",
    conversation_id: "c_1",
    kind: "bug",
    title: "Pricing toggle shows yearly twice",
    summary: "Toggling billing duplicates the yearly total.",
    steps: ["Open /pricing", "Toggle twice"],
    expected: "Monthly price",
    actual: "Yearly total twice",
    severity: "medium",
    confidence: 0.86,
    duplicate_of: null,
    status: "draft",
    github_issue_url: null,
    created_at: "2026-10-24T10:00:00Z",
    decided_at: null,
    decided_reason: null,
    page_url: "https://example.test/pricing",
    ua: "Mozilla/5.0 Chrome/130",
    viewport: "1440x900",
    ...overrides,
  };
}

export function detail(overrides: Partial<TicketDetail> = {}): TicketDetail {
  return {
    ...ticket(),
    transcript: [
      { role: "visitor", text: "Toggle is broken, reach me at [EMAIL].", redactions: { EMAIL: 1 }, created_at: "2026-10-24T10:00:00Z" },
      { role: "assistant", text: "What were you doing right before this happened?", redactions: {}, created_at: "2026-10-24T10:00:05Z" },
      { role: "visitor", text: "Toggling twice.", redactions: {}, created_at: "2026-10-24T10:00:30Z" },
    ],
    duplicates: [{ id: 3, title: "Pricing toggle shows the yearly amount twice", status: "draft", github_issue_url: null, score: 0.42 }],
    ...overrides,
  };
}

export const queueTickets: Ticket[] = [
  ticket(),
  ticket({ id: 2, site_id: 2, site: "Morning Queue app", kind: "feature", title: "Export the collections list as CSV", confidence: 0.74, steps: [], expected: null, actual: null }),
  ticket({ id: 3, kind: "bug", title: "Pricing toggle shows the yearly amount twice", confidence: 0.61, duplicate_of: 1 }),
];

export function fakeApi(overrides: Partial<AdminApi> = {}): AdminApi {
  return {
    listTickets: vi.fn(async () => ({ tickets: queueTickets, sites })),
    getTicket: vi.fn(async (id: number) => detail({ ...queueTickets.find((t) => t.id === id) })),
    approve: vi.fn(async (id: number, fields) => ticket({ id, ...fields, status: "approved", github_issue_url: `https://github.com/example/repo/issues/${100 + id}` })),
    merge: vi.fn(async (id: number, into: number) => ticket({ id, status: "merged", duplicate_of: into })),
    dismiss: vi.fn(async (id: number, reason: string) => ticket({ id, status: "dismissed", decided_reason: reason })),
    logout: vi.fn(async () => {}),
    ...overrides,
  };
}
