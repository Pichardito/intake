import type { Scrubber } from "./types";

interface VeilkitModule {
  createScrubber(wasmUrl?: string | URL): Promise<Scrubber>;
}

let pending: Promise<Scrubber> | null = null;

/**
 * Load Veilkit lazily from `veilkit-loader.js`, served next to `intake.js`.
 * The URL is computed at runtime so the IIFE stays a single file and the wasm
 * only downloads when the panel opens. Failures are not cached, so a retry
 * (the next send) attempts the download again.
 */
export function makeScrubberLoader(scriptUrl: string): () => Promise<Scrubber> {
  return () => {
    if (!pending) {
      const url = new URL("veilkit-loader.js", scriptUrl).href;
      pending = (import(/* @vite-ignore */ url) as Promise<VeilkitModule>)
        .then((mod) => mod.createScrubber())
        .catch((err: unknown) => {
          pending = null;
          throw err;
        });
    }
    return pending;
  };
}
