// Bundle-size gate: the main chunk must stay around the 25 KB gz target
// (spec §4). The wasm chunk is separate and lazy, so it is reported, not gated.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { gzipSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const dist = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");
const MAIN_BUDGET_GZ = 25 * 1024;

export async function sizes() {
  const out = {};
  for (const name of ["intake.js", "veilkit-loader.js", "veilkit_bg.wasm", "veilkit.js"]) {
    const buf = await readFile(join(dist, name));
    out[name] = { raw: buf.length, gz: gzipSync(buf, { level: 9 }).length };
  }
  return out;
}

test("dist/intake.js stays under the 25 KB gzipped budget", async () => {
  const s = await sizes();
  for (const [name, { raw, gz }] of Object.entries(s)) {
    console.log(`${name.padEnd(20)} ${String(raw).padStart(8)} B raw ${String(gz).padStart(8)} B gz`);
  }
  assert.ok(s["intake.js"].gz <= MAIN_BUDGET_GZ, `intake.js is ${s["intake.js"].gz} B gz, budget ${MAIN_BUDGET_GZ}`);
});
