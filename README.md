# intake

An embeddable feedback widget with a researcher persona. A visitor reports a bug, asks for a
feature, or leaves feedback; the persona asks up to three follow-up questions; the backend
scrubs identifiers (with [Veilkit](https://github.com/Pichardito/veilkit)) before any model or
database sees the text, classifies the conversation into a draft ticket, de-duplicates it, and
places it in a queue the owner reviews. Approved tickets become GitHub issues.

The model proposes; a human disposes. Nothing with a side effect runs without an approval.

- `crates/intake-core` — pure Rust: conversation state machine, triage schema, prompt assembly, dedup scoring.
- `crates/intake-server` — axum + sqlx + Postgres: public conversation API, admin queue API, GitHub bridge.
- `widget/` — TypeScript, one script tag, scrubs in the browser before sending.
- `console/` — React queue for the owner.
- `prompts/` — the persona and triage prompts, versioned with the code.

Design: `docs/architecture.md`. Build contract: `docs/phase2-spec.md`. Working codename; the public
name is decided later. Apache-2.0.

## Status

- `crates/intake-core` and `crates/intake-server`: built, tested (unit + `sqlx::test` integration
  against Postgres, hostile-input corpus, stubbed GitHub OAuth and issue bridge). See
  `docs/decisions.md` for the calls made and what is left out, `docs/deploy.md` to run it on
  Fly.io + Neon, or `docker/compose.yml` to self-host.
- `widget/` and `console/`: in progress on a separate branch.

## Run the server locally

```bash
cp .env.example .env            # fill the required values
cargo run -p intake-server -- admin add-site --key demo --name Demo --origin http://localhost:5173 --repo owner/repo
cargo run -p intake-server      # http://localhost:8788/v1/health
```
