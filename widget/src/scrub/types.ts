/** Redaction counts per label, e.g. `{ SSN: 1, EMAIL: 2 }`. */
export type RedactionCounts = Record<string, number>;

export interface ScrubResult {
  /** Text with every detected identifier replaced by its `[LABEL]` placeholder. */
  text: string;
  counts: RedactionCounts;
}

/**
 * Scrubs one outgoing message. The only implementation in production is
 * Veilkit (`veilkit-chunk.ts`); tests substitute a fake. There is deliberately
 * no "pass-through" scrubber: if none can be loaded, nothing is sent.
 */
export type Scrubber = (text: string) => Promise<ScrubResult>;

/**
 * The JSON shape Veilkit's `WasmEngine.redact` returns (mirrors `Redaction` /
 * `RedactedSpan` in veilkit's npm/src/types.ts). Kept local so the widget has
 * no dependency on the unpublished package; only the fields used here.
 */
export interface VeilkitRedaction {
  text: string;
  spans: Array<{ id: string; label: string; start: number; end: number; placeholder: string }>;
}
