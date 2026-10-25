import { defineConfig } from "vite";

// Main widget bundle: one IIFE, loaded by a single <script> tag. Veilkit is
// NOT in this bundle; `src/scrub/loader.ts` imports `veilkit-loader.js` (built
// by vite.veilkit.config.ts) at runtime, relative to this script's own URL.
export default defineConfig({
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
    lib: {
      entry: "src/main.ts",
      name: "Intake",
      formats: ["iife"],
      fileName: () => "intake.js",
    },
  },
});
