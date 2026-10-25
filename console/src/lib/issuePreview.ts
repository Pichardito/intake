import type { Ticket, TriageFields } from "../types";

export interface IssuePreview {
  title: string;
  labels: string[];
  body: string;
}

/**
 * The GitHub issue the server will create on approve (phase2-spec §3.6),
 * rendered from the owner's edited fields so the preview is what ships.
 * The server remains the source of truth; this is a faithful copy for review.
 */
export function buildIssuePreview(ticket: Ticket, fields: TriageFields, queueUrl: string): IssuePreview {
  const sections: string[] = [`## Summary\n${fields.summary.trim()}`];

  if (fields.kind === "bug" && fields.steps.length > 0) {
    sections.push(`## Steps\n${fields.steps.map((s, i) => `${i + 1}. ${s}`).join("\n")}`);
  }

  const expected = fields.expected?.trim();
  const actual = fields.actual?.trim();
  if (expected || actual) {
    const lines = [];
    if (expected) lines.push(`**Expected:** ${expected}`);
    if (actual) lines.push(`**Actual:** ${actual}`);
    sections.push(`## Expected / Actual\n${lines.join("\n")}`);
  }

  const context: string[] = [];
  if (ticket.page_url) context.push(`- Page: ${ticket.page_url}`);
  if (ticket.ua) context.push(`- Browser: ${ticket.ua}`);
  if (ticket.viewport) context.push(`- Viewport: ${ticket.viewport}`);
  sections.push(`## Context\n${context.length ? context.join("\n") : "- (not provided)"}`);

  sections.push(`> Visitor-provided content, untrusted. Queue entry: ${queueUrl}`);

  return {
    title: `[${fields.kind}] ${fields.title.trim()}`,
    labels: ["intake", fields.kind],
    body: sections.join("\n\n") + "\n",
  };
}
