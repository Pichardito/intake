import { copyFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

// Lazy Veilkit chunk: an ES module (`dist/veilkit-loader.js`) that imports the
// wasm-bindgen glue (`dist/veilkit.js`), which in turn fetches the binary
// (`dist/veilkit_bg.wasm`) relative to its own URL. All three sit next to
// intake.js; the main bundle imports the loader only when the panel opens.
//
// The glue is kept external and copied verbatim rather than bundled: Vite's
// library mode inlines `new URL("…wasm", import.meta.url)` assets as base64,
// which would turn the lazy chunk into a 1.2 MB data URL.
const vendorDir = fileURLToPath(new URL("./vendor/veilkit/", import.meta.url));
const glue = `${vendorDir}veilkit.js`;

const externalGlue: Plugin = {
  name: "intake:external-veilkit-glue",
  enforce: "pre",
  resolveId(source, importer) {
    if (!importer || !source.endsWith("vendor/veilkit/veilkit.js")) return null;
    return { id: "./veilkit.js", external: true };
  },
  async closeBundle() {
    const dist = fileURLToPath(new URL("./dist/", import.meta.url));
    await copyFile(glue, `${dist}veilkit.js`);
    await copyFile(`${vendorDir}veilkit_bg.wasm`, `${dist}veilkit_bg.wasm`);
  },
};

export default defineConfig({
  plugins: [externalGlue],
  build: {
    outDir: "dist",
    emptyOutDir: false,
    target: "es2022",
    sourcemap: false,
    lib: {
      entry: "src/scrub/veilkit-chunk.ts",
      formats: ["es"],
      fileName: () => "veilkit-loader.js",
    },
  },
});
