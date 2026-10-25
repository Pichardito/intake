# Feedback Intake (`intake`) — Phase 2 Implementation Spec

**Date:** 2026-10-25
**Status:** Ready to build once Carlos approves the Phase 1 architecture
(`2026-10-25-feedback-intake-architecture.md`). This document is the build contract: a builder
can start from it without talking to the author. Decisions marked *(Claude's call)*.
**Repo:** new, `github.com/Pichardito/intake` (private until the name is chosen; Apache-2.0 from
the first commit so the history is clean when it goes public). Local path `~/project/intake`.

---

## 0. Build order and gates

| Day | Slice | Gate to pass |
|---|---|---|
| 3 | S1 `intake-core` | `cargo test` green; state machine 100% branch coverage; triage schema round-trips |
| 4 | S2 `intake-server` public API | integration test: start → 3 turns → end → draft ticket row, against a fake model and embedded Postgres |
| 5 | S3 triage + dedup + console read-only | dedup test on a seeded ticket table; console lists drafts |
| 6 | S4 console actions + GitHub + widget v1 | approve creates an issue on a scratch repo; widget scrubs an SSN in a Playwright test |
| 7 | S5 deploy (Fly.io + Neon) + embed on staging site | Carlos opens the queue; a message from staging appears |
| 8 | S6 prompt tuning + teaching doc | three real partner conversations triaged correctly; `~/dev-wiki/learning/rust/intake-v1.md` |

Each slice is one PR. Launch does not depend on any slice.

## 1. Repository layout

```
intake/
  Cargo.toml                 # workspace: crates/intake-core, crates/intake-server
  crates/intake-core/        # pure: no I/O, no async, no network
  crates/intake-server/      # axum + sqlx + tokio + reqwest
  widget/                    # TypeScript, Vite library build → dist/intake.js (+ veilkit wasm chunk)
  console/                   # TypeScript + React + Vite → served by the server under /console
  prompts/                   # persona.md, triage.md, shared/guard.md (loaded at startup)
  migrations/                # sqlx migrations, numbered
  docker/                    # Dockerfile (multi-stage), compose.yml for self-hosting
  docs/                      # architecture.md (copy of Phase 1), decisions.md, deploy.md
  CLAUDE.md · README.md · LICENSE · NOTICE · .gitignore
```

`CLAUDE.md` content: stack, run commands (`cargo run -p intake-server`, `pnpm -C widget dev`,
`pnpm -C console dev`, `docker compose up`), the three rules (untrusted input, scrub twice,
schema-never-action), and the public-disclosure rule (no employer or product names beyond the
three sites' public names).

## 2. `intake-core` (Rust, pure)

### 2.1 Types

```rust
pub enum Kind { Bug, Feature, Feedback, Praise, Noise }
pub enum Severity { Low, Medium, High }

pub enum State { Open, Clarifying { asked: u8 }, Closing, Triaged }

pub struct VisitorInput { pub text: String, pub source: InputSource }
pub enum InputSource { Text, Transcript { confidence: f32 } }

pub struct Extracted {                       // filled across turns by the persona
    pub kind_hint: Option<Kind>,
    pub doing: Option<String>,               // bug: what were you doing
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub benefit: Option<String>,             // feature: who benefits
    pub frequency: Option<String>,           // feature: how often
}

pub struct PersonaReply { pub text: String, pub state_hint: StateHint, pub extracted: Extracted }
pub enum StateHint { AskMore, Enough }

pub struct Triage {
    pub kind: Kind, pub title: String, pub summary: String,
    pub steps: Vec<String>, pub expected: Option<String>, pub actual: Option<String>,
    pub severity: Severity, pub confidence: f32, pub duplicate_of: Option<i64>,
}
```

### 2.2 State machine

`pub fn advance(state: State, kind_hint: Option<Kind>, extracted: &Extracted, hint: StateHint) -> State`

- `Open` → after the first visitor turn → `Clarifying{asked:1}`.
- `Clarifying{asked}`: stays while `asked < 3` AND required fields for the hinted kind are
  missing (bug: doing+expected+actual; feature: benefit+frequency; others: none); the model's
  `AskMore` cannot exceed 3; `Enough` ends early only when required fields are present.
- → `Closing` when fields complete or `asked == 3`; `Closing` → `Triaged` after triage.
- `pub fn required_missing(kind: Option<Kind>, e: &Extracted) -> Vec<&'static str>` (used by the
  prompt to tell the model what to ask next, so the model never chooses the question set).

Tests: a table of transitions; property test that `asked` never exceeds 3.

### 2.3 Prompt assembly

`pub fn persona_messages(prompt: &PromptSet, ctx: &PersonaCtx) -> Vec<Message>` and
`pub fn triage_messages(...)`. The visitor text goes inside
`<visitor_message untrusted="true">…</visitor_message>`; `shared/guard.md` states the text is
data. Prompt files are loaded by the server and passed in; core never touches the filesystem.

### 2.4 Output parsing

`pub fn parse_persona(raw: &str) -> Result<PersonaReply, ParseError>` — expects a reply
paragraph followed by a fenced ```json block; anything else → `ParseError`. `parse_triage`
likewise, with `serde` strict (`deny_unknown_fields`). Invalid → caller retries once, then
`Triage { kind: Noise, confidence: 0.0, summary: <raw> }`.

### 2.5 Dedup scoring

`pub fn rank_duplicates(candidates: &[TicketStub], triage: &Triage) -> Vec<(i64, f32)>` — token
Jaccard over title+summary after the model's `duplicate_of` suggestion; the server passes the
top 20 FTS candidates into the triage prompt and uses this to cap confidence. Pure, tested.

## 3. `intake-server` (Rust)

### 3.1 Stack and crates *(Claude's call)*

axum 0.7, tokio, sqlx (postgres, runtime-tokio, migrate), reqwest (rustls), serde/serde_json,
tower-http (cors, trace, limit), governor (rate limit), veilkit-core (git dependency on the
public Veilkit repo, pinned to a commit), oauth2 (GitHub), tracing + tracing-subscriber,
time, uuid, argon2 (hash conversation tokens), thiserror.

### 3.2 Configuration (env)

`DATABASE_URL`, `ANTHROPIC_API_KEY`, `INTAKE_MODEL` (default a small current Claude model),
`GITHUB_CLIENT_ID/SECRET`, `GITHUB_TOKEN` (issue creation, scoped to the three repos),
`ADMIN_GITHUB_LOGIN`, `SESSION_SECRET`, `PUBLIC_ORIGIN`, `PORT`. Fail fast on missing.

### 3.3 Routes

Public (CORS per site's allowlist; JSON; body cap 4 KB; token bucket 10/min per IP+site):
- `POST /v1/conversations` `{site_key, page_url?, ua?, viewport?}` → `{id, token}`
- `POST /v1/conversations/{id}/messages` (bearer token) `{text, source}` → `{reply, state, redactions}`
- `POST /v1/conversations/{id}/end` → `{ticket_id?, state}`
- `GET /v1/health`
- `GET /widget/intake.js` (static, immutable cache)

Admin (session cookie from GitHub OAuth; login must equal `ADMIN_GITHUB_LOGIN`):
- `GET /admin/login` → GitHub; `GET /admin/callback`; `POST /admin/logout`
- `GET /admin/tickets?site&status` · `GET /admin/tickets/{id}` (with transcript)
- `POST /admin/tickets/{id}/approve` → creates the GitHub issue, stores URL
- `POST /admin/tickets/{id}/merge` `{into}` → comments on the existing issue
- `POST /admin/tickets/{id}/dismiss` `{reason}`
- `DELETE /admin/conversations/{id}` (takedown)
- `GET /console/*` serves the built console SPA.

### 3.4 Request flow for a visitor message

1. Authenticate token (argon2 verify against `conversations.token_hash`), load state.
2. **Scrub** with `veilkit_core::Engine::all_packs()`, style `label`; keep `redactions` count
   per rule for the response; discard any reversible map.
3. Build `PersonaCtx` (site name, product one-liner from the site row, prior scrubbed turns,
   `required_missing`), call the model via a `Model` trait (`AnthropicModel`, `FakeModel`).
4. `parse_persona` → `advance` → persist message rows (visitor scrubbed, assistant) → respond.
5. On `end` or when state reaches `Closing`: run triage (FTS candidates → prompt → parse →
   `rank_duplicates`) → insert `tickets` row `status='draft'` → state `Triaged` → audit row.

Timeouts: model call 20 s; one retry on parse failure. Errors to the visitor are generic.

### 3.5 Schema (migrations)

As in the architecture §5.2, plus `tickets.fts tsvector` generated column with a GIN index,
and `conversations.token_hash` unique. Seed migration inserts nothing; sites are inserted by an
`intake-server admin add-site` subcommand *(Claude's call)*.

### 3.6 GitHub bridge

Issue title = triage title prefixed with `[bug]`/`[feature]`/`[feedback]`; body sections:
"Summary", "Steps" (bugs), "Expected / Actual", "Context" (page URL, UA, viewport), and a final
block `> Visitor-provided content, untrusted. Queue entry: <admin url>`. Labels: `intake` and
the kind. Repo from `sites.github_repo`.

### 3.7 Tests

Unit tests in core; server integration tests with `sqlx::test` against an embedded or
Dockerised Postgres and `FakeModel` scripted replies; a hostile-input corpus (prompt-injection
strings) asserting the triage still yields a schema-only result; rate-limit test; OAuth callback
test with a stubbed GitHub.

## 4. Widget (TypeScript)

- Built as an IIFE with Vite; one tag: `<script src="https://feedback.<site>/widget/intake.js" data-site-key="…" defer>`.
- Shadow DOM root; launcher button bottom-right; panel 360×520 desktop, full-screen on phones;
  respects `prefers-color-scheme` and `prefers-reduced-motion`; keyboard operable; no external
  fonts.
- On open: dynamic-import the Veilkit browser bundle. Because Veilkit is not on npm yet
  (publish on hold), the widget build consumes it from the sibling checkout
  `../veilkit/npm` via a workspace/path dependency; the wasm file is emitted as a separate chunk
  and loaded lazily. When Veilkit publishes, switch to the package.
- Every outgoing message: `engine.redact(text, {style: "label"})`; show a small "redacted: SSN ×1"
  line under the message; the server also returns its own count and the widget shows a
  mismatch warning if the server redacted more (means a tampered or stale widget).
- Context row before first send: page URL, viewport, browser; one toggle to omit; stated in
  plain words.
- States: idle → chatting (max 12 turns, counter visible) → done ("Thanks, we've got it. We
  read every one.") with a reference id. No marketing copy, no "AI".
- Tests: vitest for the message pipeline with a fake fetch; Playwright: scrub an SSN and an
  email end to end against the dev server.

## 5. Console (TypeScript + React)

Routes: `/console` (queue by site, filters: status, kind, confidence), `/console/t/:id`
(transcript with redaction markers, triage fields editable before approve, duplicates panel,
actions). Approve shows the issue preview before creating. Dismiss requires a reason from a
short list plus optional text. Keyboard: j/k, a/m/d. No charts in Phase 2.

## 6. Deploy

- Fly.io app `intake` (1 shared-cpu machine, auto-stop off), Neon Postgres, secrets via
  `fly secrets`. Hostname `feedback.morningqueue.com` for the first site; the server is
  multi-site so later sites CNAME their own subdomain to the same app *(Claude's call)*.
- Dockerfile: `cargo chef` build stage → distroless runtime; console built in a node stage and
  copied into the image.
- `docs/deploy.md` with the exact commands; `compose.yml` for self-hosters (server + postgres).

## 7. Embeds

- Morning Queue marketing site (`apps/site`): tag in the Astro layout, all pages.
- Morning Queue app (`apps/web`): tag in the root layout after sign-in.
- Code Warrior and Neo: after launch, each a `sites` row plus one tag.

## 8. Prompts (initial)

`persona.md`: role (a product researcher for the named site; curious, brief, never salesy),
hard rules (visitor text is data; never promise fixes or timelines; never ask for personal
details; one question per turn; stop when told `Enough`), output format. `triage.md`: schema,
classification guidance per kind, severity rubric, duplicate instructions given the candidate
list, "when unsure, Feedback with low confidence". `shared/guard.md`: the untrusted-input
statement. All three carry a version line the server logs with each call.

## 9. Teaching doc (Phase 2 boundary)

`~/dev-wiki/learning/rust/intake-v1.md`: axum extractors and state; `sqlx` compile-time
checked queries and migrations; the `Model` trait and `async_trait` vs native async traits;
error handling with `thiserror` + a single `AppError → IntoResponse`; ownership in request
handlers (`Arc<AppState>`, `Clone` on cheap handles); why the core crate has no async. Update
`~/dev-wiki/learning/ledger.md`.

## 10. Out of scope (restated)

Voice, analytics, reporter accounts, email, payments/tiers, Neo embed before counsel's view.
