import { describe, expect, it, vi } from "vitest";
import { createAdminApi } from "../src/api";

function fetchReturning(status: number, body: unknown = {}) {
  return vi.fn(async () => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } }));
}

describe("admin api client", () => {
  it("builds the queue URL from the filters and sends the session cookie", async () => {
    const fetchFn = fetchReturning(200, { tickets: [], sites: [] });
    const api = createAdminApi({ fetchFn, onUnauthorized: () => {} });
    await api.listTickets({ site: "1", status: "draft", kind: "bug", min_confidence: "0.5" });
    const [url, init] = fetchFn.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("/admin/tickets?site=1&status=draft&kind=bug&min_confidence=0.5");
    expect(init.credentials).toBe("same-origin");
  });

  it("omits empty filters", async () => {
    const fetchFn = fetchReturning(200, { tickets: [], sites: [] });
    const api = createAdminApi({ fetchFn, onUnauthorized: () => {} });
    await api.listTickets({ status: "draft", kind: "" });
    expect((fetchFn.mock.calls[0] as unknown as [string])[0]).toBe("/admin/tickets?status=draft");
  });

  it("calls onUnauthorized and rejects on a 401", async () => {
    const onUnauthorized = vi.fn();
    const api = createAdminApi({ fetchFn: fetchReturning(401), onUnauthorized });
    await expect(api.getTicket(1)).rejects.toThrow(/unauthorized/i);
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
  });

  it("posts approve with the edited fields, merge with the target, dismiss with reason and note", async () => {
    const fetchFn = fetchReturning(200, { ticket: {} });
    const api = createAdminApi({ fetchFn, onUnauthorized: () => {} });
    const fields = { kind: "bug" as const, title: "t", summary: "s", steps: [], expected: null, actual: null, severity: "low" as const };
    await api.approve(3, fields);
    await api.merge(3, 1);
    await api.dismiss(3, "noise", "spam");
    const calls = fetchFn.mock.calls as unknown as [string, RequestInit][];
    expect(calls[0]?.[0]).toBe("/admin/tickets/3/approve");
    expect(JSON.parse(String(calls[0]?.[1].body))).toEqual({ fields });
    expect(calls[1]?.[0]).toBe("/admin/tickets/3/merge");
    expect(JSON.parse(String(calls[1]?.[1].body))).toEqual({ into: 1 });
    expect(calls[2]?.[0]).toBe("/admin/tickets/3/dismiss");
    expect(JSON.parse(String(calls[2]?.[1].body))).toEqual({ reason: "noise", note: "spam" });
  });

  it("surfaces the server's error message on other failures", async () => {
    const api = createAdminApi({ fetchFn: fetchReturning(409, { error: "ticket is approved" }), onUnauthorized: () => {} });
    await expect(api.merge(1, 2)).rejects.toThrow("ticket is approved");
  });
});
