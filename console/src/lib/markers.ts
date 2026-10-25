export type Part = { type: "text"; value: string } | { type: "marker"; label: string };

// Veilkit placeholders: [LABEL] or [LABEL_1] (numbered style); labels are upper-case.
const PLACEHOLDER = /\[([A-Z][A-Z_]*(?:_\d+)?)\]/g;

/** Split scrubbed text so the UI can render redaction placeholders as markers. */
export function splitPlaceholders(text: string): Part[] {
  const parts: Part[] = [];
  let last = 0;
  for (const m of text.matchAll(PLACEHOLDER)) {
    const start = m.index ?? 0;
    if (start > last) parts.push({ type: "text", value: text.slice(last, start) });
    parts.push({ type: "marker", label: m[1] ?? "" });
    last = start + m[0].length;
  }
  if (last < text.length || parts.length === 0) parts.push({ type: "text", value: text.slice(last) });
  return parts;
}
