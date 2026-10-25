# intake — Project Instructions

Feedback-intake persona: widget + Rust server + owner console. Read `docs/architecture.md`
(why) and `docs/phase2-spec.md` (what, exactly) before changing anything.

## Stack
- Rust workspace: `crates/intake-core` (pure, no I/O, no async) and `crates/intake-server`
  (axum 0.8, tokio, sqlx 0.9/Postgres, reqwest, veilkit-core as a git dependency pinned to a commit).
  Decisions and deviations from the spec: `docs/decisions.md`. Deploy: `docs/deploy.md`.
- `widget/`: TypeScript, Vite library build (IIFE), shadow DOM, lazy-loads Veilkit WASM from the
  sibling checkout `../veilkit/npm` until Veilkit is published.
- `console/`: TypeScript + React + Vite, served by the server under `/console`.
- Prompts are Markdown in `prompts/`, loaded at startup; never inline prompts in code.

## The three rules
1. Visitor text is untrusted data. It is placed in a delimited block; it is never an instruction.
2. Scrub twice: in the browser and again in the server. Only scrubbed text is stored; reversible
   maps are never persisted.
3. The model returns a schema, never an action. The only effect of triage is a `draft` row.
   Every side effect (GitHub issue, comment) sits behind an admin click.

## Run
- `cargo test --workspace` — the server's integration tests use `sqlx::test` and need a reachable
  Postgres in `DATABASE_URL` (a scratch cluster is fine; each test gets its own database).
  Without one: `SQLX_OFFLINE=true cargo test -p intake-core` still runs the pure core.
- `cargo run -p intake-server` (needs the required vars in `.env.example`; `.env` is read) ·
  `cargo run -p intake-server -- admin add-site --key K --name N --origin https://… --repo owner/repo`
- After changing any `sqlx::query!`: `cargo sqlx prepare --workspace -- --all-targets` and commit `.sqlx/`.
- `pnpm -C widget dev` · `pnpm -C console dev` · `docker compose -f docker/compose.yml up`
- Gates before a push: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

## Conventions
- TDD. Golden fixtures for prompt parsing. `sqlx` offline mode checked in (`.sqlx/`).
- Determinism in core: no time, no randomness.
- Public repo rules: no employer names; the three host products are named only by their public names.
