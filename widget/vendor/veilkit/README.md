# Vendored Veilkit (wasm, `--target web`)

Generated output of `wasm-pack build --release --target web` for the
`veilkit-wasm` crate in the sibling checkout `../../../veilkit`. Vendored
because Veilkit is not published to npm yet and the widget's lazy chunk needs
the `web` target (an `init(url)` function) rather than the `bundler` target the
npm package ships.

- `veilkit.js` — wasm-bindgen glue, copied verbatim to `dist/veilkit.js`
- `veilkit_bg.wasm` — the engine, copied to `dist/veilkit_bg.wasm` (~893 KB raw, ~370 KB gz)
- `veilkit.d.ts` — types for the glue

Regenerate with `pnpm -C widget build:veilkit` (needs the Rust toolchain,
`wasm32-unknown-unknown`, `wasm-pack`, and the Veilkit checkout at
`../veilkit` next to this repo, or `VEILKIT_DIR=/path/to/veilkit`). The widget has no package dependency on
Veilkit: the engine types come from `veilkit.d.ts` here and the `Redaction`
JSON shape is mirrored in `src/scrub/types.ts`. When Veilkit publishes, replace
this directory with the package and delete `build:veilkit`.

Do not edit these files by hand. Licensed Apache-2.0, same as Veilkit.
