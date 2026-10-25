import type { RedactionCounts } from "./types";

/** Count redaction spans per label (Veilkit's `Redaction.spans`). */
export function countsFromSpans(spans: ReadonlyArray<{ label: string }>): RedactionCounts {
  const counts: RedactionCounts = {};
  for (const span of spans) {
    counts[span.label] = (counts[span.label] ?? 0) + 1;
  }
  return counts;
}

/** "redacted: SSN ×2, EMAIL ×1" — highest count first, then alphabetical; "" when empty. */
export function formatRedactions(counts: RedactionCounts): string {
  const entries = Object.entries(counts).filter(([, n]) => n > 0);
  if (entries.length === 0) return "";
  entries.sort(([la, a], [lb, b]) => b - a || la.localeCompare(lb));
  return `redacted: ${entries.map(([label, n]) => `${label} ×${n}`).join(", ")}`;
}

/**
 * True when the server reports more redactions than the browser made for any
 * label. That means the visitor's page is running a stale or tampered widget
 * and raw identifiers may have left the browser.
 */
export function serverRedactedMore(
  client: RedactionCounts,
  server: RedactionCounts | undefined,
): boolean {
  if (!server) return false;
  for (const [label, n] of Object.entries(server)) {
    if (n > (client[label] ?? 0)) return true;
  }
  return false;
}
