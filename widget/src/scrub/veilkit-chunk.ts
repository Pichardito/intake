// The lazy chunk. Built separately (vite.veilkit.config.ts) into
// `dist/veilkit-loader.js` + `dist/veilkit_bg.wasm`; the main bundle imports
// it by URL when the panel opens (see loader.ts).
//
// Veilkit is consumed from the vendored `--target web` wasm-pack output in
// `vendor/veilkit/` (see vendor/veilkit/README.md). The `veilkit` npm package
// in the sibling checkout is used for its TypeScript types only: its runtime
// entry imports the wasm as an ES module, which needs a bundler wasm plugin
// and cannot be split out of an IIFE.
import type { Redaction } from "veilkit";
import init, { createEngine, type WasmEngine } from "../../vendor/veilkit/veilkit.js";
import { countsFromSpans } from "./summary";
import type { Scrubber } from "./types";

const PACKS = ["pii", "phi-us", "secrets", "finance"];
const OPTIONS = JSON.stringify({ style: "label" });

let engine: WasmEngine | null = null;

/** Instantiate the wasm once and return a scrubber bound to it. */
export async function createScrubber(wasmUrl?: string | URL): Promise<Scrubber> {
  if (!engine) {
    await init(wasmUrl ? { module_or_path: wasmUrl } : undefined);
    engine = createEngine(PACKS, null);
  }
  const e = engine;
  return async (text: string) => {
    const red = JSON.parse(e.redact(text, OPTIONS)) as Redaction;
    return { text: red.text, counts: countsFromSpans(red.spans) };
  };
}
