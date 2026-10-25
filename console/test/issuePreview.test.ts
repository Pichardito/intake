import { describe, expect, it } from "vitest";
import { buildIssuePreview } from "../src/lib/issuePreview";
import type { Ticket } from "../src/types";

const base: Ticket = {
  id: 7,
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
  confidence: 0.8,
  duplicate_of: null,
  status: "draft",
  github_issue_url: null,
  created_at: "2026-10-24T10:00:00Z",
  decided_at: null,
  decided_reason: null,
  page_url: "https://example.test/pricing",
  ua: "Mozilla/5.0 Chrome/130",
  viewport: "1440x900",
};

describe("buildIssuePreview (spec §3.6)", () => {
  it("prefixes the title with the kind and labels intake + kind", () => {
    const p = buildIssuePreview(base, base, "https://feedback.example.test/console/t/7");
    expect(p.title).toBe("[bug] Pricing toggle shows yearly twice");
    expect(p.labels).toEqual(["intake", "bug"]);
  });

  it("renders Summary, Steps, Expected / Actual, Context and the untrusted footer", () => {
    const p = buildIssuePreview(base, base, "https://feedback.example.test/console/t/7");
    expect(p.body).toContain("## Summary\nToggling billing duplicates the yearly total.");
    expect(p.body).toContain("## Steps\n1. Open /pricing\n2. Toggle twice");
    expect(p.body).toContain("## Expected / Actual\n**Expected:** Monthly price\n**Actual:** Yearly total twice");
    expect(p.body).toContain("## Context\n- Page: https://example.test/pricing\n- Browser: Mozilla/5.0 Chrome/130\n- Viewport: 1440x900");
    expect(p.body.trimEnd().endsWith("> Visitor-provided content, untrusted. Queue entry: https://feedback.example.test/console/t/7")).toBe(true);
  });

  it("uses the edited fields, not the stored ticket", () => {
    const p = buildIssuePreview(base, { ...base, kind: "feature", title: "Export CSV", steps: [] }, "u");
    expect(p.title).toBe("[feature] Export CSV");
    expect(p.labels).toEqual(["intake", "feature"]);
    expect(p.body).not.toContain("## Steps");
  });

  it("omits Steps for non-bugs and Expected / Actual when both are empty", () => {
    const p = buildIssuePreview(base, { ...base, kind: "feedback", expected: null, actual: null }, "u");
    expect(p.body).not.toContain("## Steps");
    expect(p.body).not.toContain("## Expected / Actual");
  });

  it("leaves out context lines the visitor chose not to send", () => {
    const p = buildIssuePreview({ ...base, page_url: null, ua: null, viewport: null }, base, "u");
    expect(p.body).toContain("## Context\n- (not provided)");
  });
});
