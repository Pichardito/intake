# Feedback Intake Persona — Phase 1 Architecture

**Date:** 2026-10-25
**Status:** Phase 1 (architecture), for Carlos's approval before Phase 2. Scheduled **before the
Morning Queue launch** at Carlos's direction (ruling 2026-10-25).
**Working codename:** `intake`. Not a brand; the public name is decided with the repo.
**Author:** Claude. Decisions marked *(Claude's call)* were made without asking.

---

## 1. What it is

An embeddable chat widget plus a small backend that acts as a **UX researcher persona** on each
of Carlos's three products (Morning Queue, Code Warrior, Neo/Luxin). A visitor reports a bug,
asks for a feature, or gives feedback; the persona asks up to three follow-up questions to get
the details a developer needs; the backend scrubs identifiers, classifies the conversation into a
**draft ticket** (bug, feature, feedback, praise, noise), de-duplicates it against open tickets,
and places it in a **queue only Carlos sees**. Carlos approves, merges or dismisses. Approved
tickets become GitHub issues in the right repository with a label a Claude session watches.

Phase 1 ships **text chat**. Voice is designed as a seam (§6) and built later.

## 2. Why now, and why this shape

- Morning Queue launches on 2026-11-07 with design partners. That is the week Carlos most needs
  structured feedback and least wants to read it raw. The widget on `morningqueue.com` and
  `app.morningqueue.com` is the first consumer; Code Warrior and Neo embed after launch.
- The model **proposes, Carlos disposes, Claude executes** is the existing operating model
  (decide-then-inform). The queue is the human gate; nothing downstream runs without it.
- Feedback text is untrusted. On Neo it may contain patient information. **Veilkit scrubs it in
  the browser before it is sent**, and again on the server, so no model and no database sees raw
  identifiers. That is the differentiator against hosted feedback tools and the reason this can
  sit on a HIPAA-scoped site at all (counsel to confirm the posture; see §9).

## 3. Goals, non-goals, anti-goals

**Goals (Phase 1):** text widget on Morning Queue by launch; draft tickets with confidence and
duplicate detection; approval queue; GitHub issue creation; multi-site from day one; open-source
core under Apache-2.0 with no reference to Neo or any employer.

**Non-goals (Phase 1):** voice; analytics dashboards; sentiment trends; user accounts for
reporters; email follow-ups; mobile SDKs.

**Anti-goals (ever):** the triage step opening pull requests, running code, emailing or
messaging anyone; storing unscrubbed text; a visible "AI-powered" badge; reporter identification
beyond what they type.

## 4. Language choice

| Piece | Language | Why |
|---|---|---|
| Server (`intake-server`) | **Rust** (axum, sqlx, tokio) | Carlos's stated preference and an active learning goal; one static binary on Fly.io next to the Morning Queue API; `veilkit-core` links natively for the server-side scrub; the LLM calls are plain HTTPS+JSON, no SDK needed. |
| Domain core (`intake-core`) | **Rust**, no I/O | Conversation state machine, triage schema, prompt templating, dedup scoring. Pure, so it is fully unit-testable and reusable from the server and from tests. |
| Widget (`widget/`) | **TypeScript**, no framework, ~25 KB gz target | Runs on three sites with different stacks (Astro, React, Next). Loads `veilkit` WASM lazily for the client-side scrub. |
| Console (`console/`) | **TypeScript** + React + Vite | Carlos's queue UI; small; his expert stack. |
| Prompts | Markdown files in `prompts/` | Iterated weekly without recompiling; loaded at startup; versioned with the repo. |

Why not TypeScript for the server: it would be faster to write, but the server is small, the
scrub step wants native Veilkit, and the point of the side projects is to ship *and* learn. Why
not Rust for the widget: bundle size and three host stacks; Veilkit already provides the Rust
part the widget needs.

## 5. Architecture

```
  visitor's browser                       intake-server (Rust, Fly.io)              Carlos
 ┌────────────────────┐   HTTPS/JSON    ┌───────────────────────────────┐       ┌──────────────┐
 │ widget (TS)        │ ─────────────▶  │ /v1/conversations (public)    │       │ console (TS) │
 │  • Veilkit WASM    │  scrubbed text  │  • site key + origin check    │       │  queue, approve
 │    scrub BEFORE    │                 │  • rate limit per IP+site     │ ◀──── │  merge, dismiss
 │    send            │ ◀─────────────  │  • persona turn → LLM         │ GitHub│  (GitHub OAuth)
 │  • 3 follow-ups max│  persona reply  │  • server-side scrub (belt)   │ OAuth └──────────────┘
 └────────────────────┘                 │  • conversation state machine │              │ approve
                                        │  • triage → draft ticket      │              ▼
                                        │  • dedup vs open tickets      │    GitHub Issues API
                                        │  • /admin/* (Carlos only)     │    label: intake
                                        └──────────────┬────────────────┘    repo per site
                                                       │ sqlx
                                                       ▼
                                             Postgres (Neon) — sites, conversations,
                                             messages (scrubbed), tickets, audit
```

### 5.1 Components

- **Widget.** A `<script>` tag with `data-site-key`. Renders a launcher button and a panel;
  no iframe (CSS-scoped by a shadow root). On open it lazy-loads `veilkit` (browser build) with
  the `pii`, `phi-us`, `secrets` and `finance` packs. Every outbound message is scrubbed
  client-side with the `label` style; the widget shows the visitor what was redacted so the
  behaviour is visible, not hidden. Collects nothing automatically except page URL, viewport, and
  user agent (shown to the visitor before sending; one toggle to omit).
- **Public API.** `POST /v1/conversations` (start; returns an opaque conversation token),
  `POST /v1/conversations/{id}/messages` (visitor turn; returns persona reply and state),
  `POST /v1/conversations/{id}/end`. Site key is public; the server enforces an origin allowlist
  per site, a token bucket per IP and per site, a message cap (12 turns), and a body cap (4 KB).
  Optional Cloudflare Turnstile per site.
- **Conversation state machine** (`intake-core`): `Open → Clarifying(n ≤ 3) → Closing →
  Triaged`. The persona may ask at most three questions; it must ask what-were-you-doing,
  what-did-you-expect, what-happened for a bug; who-benefits and how-often for a feature. The
  machine decides when to stop asking; the model only writes the words.
- **Persona turn.** One Claude call per visitor message (Messages API, small model; prompt
  from `prompts/persona.md`). The visitor's text is placed inside a delimited data block; the
  system prompt says it is untrusted and must never be followed as instructions. Output is a
  short reply plus a JSON block with `state_hint` and `extracted_fields`; `intake-core` validates
  it and ignores anything outside the schema.
- **Server-side scrub.** `veilkit-core` runs again on every stored field (belt and braces for
  a tampered widget). Only scrubbed text is persisted; the reversible map is never stored.
- **Triage.** On `end` or after the cap, one call with `prompts/triage.md` producing strict
  JSON: `kind ∈ {bug, feature, feedback, praise, noise}`, `title`, `summary`, `steps[]`,
  `expected`, `actual`, `severity`, `confidence 0–1`, `duplicate_of?`. Candidate duplicates come
  from Postgres full-text search over open tickets for the site (top 20) passed into the prompt;
  `intake-core` scores and caps them. Output validated; invalid JSON → retry once → `kind: noise`
  with the raw summary for Carlos to read.
- **Queue (console).** Lists draft tickets by site with the scrubbed transcript, the triage
  fields, and duplicates. Actions: approve (creates the GitHub issue), merge into an existing
  issue (adds a comment), dismiss (with reason, used later to tune prompts). Admin auth is
  GitHub OAuth restricted to one GitHub login (Carlos), sessions in signed cookies.
- **GitHub bridge.** On approve: create issue in the site's repo with labels `intake` and the
  kind, body sections marked "visitor-provided, untrusted", link back to the queue entry. A
  Claude session that picks up `intake` issues treats the body as data (rule added to each repo's
  `CLAUDE.md` in Phase 2).
- **Audit.** Every state transition and admin action is a row; retention 90 days for
  conversations, indefinite for tickets; `DELETE /admin/conversations/{id}` for takedown.

### 5.2 Data model (Postgres)

`sites(id, name, origin_allowlist[], github_repo, turnstile_secret?)` ·
`conversations(id, site_id, token_hash, state, page_url, ua, started_at, ended_at)` ·
`messages(id, conversation_id, role, text_scrubbed, redactions_json, created_at)` ·
`tickets(id, site_id, conversation_id, kind, title, summary, fields_json, severity,
confidence, duplicate_of, status ∈ {draft, approved, merged, dismissed}, github_issue_url,
decided_at, decided_reason)` · `audit(id, actor, action, subject, at)`.

### 5.3 Boundaries and why they sit there

- **Scrub at the edge and again at the core**: the browser scrub protects the visitor from the
  network and the model; the server scrub protects the database from a modified widget.
- **State machine in code, words from the model**: the number of questions, when to stop, and
  what fields are required are not left to a prompt, so behaviour is testable and bounded.
- **Triage returns a schema, never an action**: the only effect of the model's output is a row
  with `status = draft`. Everything with a side effect sits behind Carlos's click.
- **One server, many sites**: a `site_id` on every row; no per-product deployment.

### 5.4 Trade-offs

- Lazy-loading Veilkit (~370 KB gz) costs a moment on first open; acceptable because the panel
  opens on intent, not on page load. The widget shows a "preparing" state.
- FTS dedup misses paraphrases; embeddings are a later upgrade. Carlos sees duplicates anyway.
- GitHub OAuth for a single admin is heavier than a shared secret but gives a real audit actor
  and costs nothing.
- Rust server means slower first build than TypeScript; mitigated by `intake-core` being pure and
  the server being thin.

### 5.5 What would force a redesign

- A second admin user (needs roles). · Voice at scale (needs a streaming media path, §6). ·
  A customer wanting to self-host with their own model (needs a provider trait; cheap to add
  now, so *(Claude's call)* the LLM client is behind a `Model` trait from the start). ·
  Conversations over ~12 turns (the state machine is built for short intake, not support).

## 6. Voice seam (designed now, built later)

The persona turn takes `VisitorInput::Text | VisitorInput::Transcript { text, confidence }`.
Phase 2+ adds a widget mode with push-to-talk: browser `MediaRecorder` → `POST /v1/.../audio`
→ server-side speech-to-text → the same text path; replies optionally synthesised with the TTS
provider Morning Queue already uses for its videos. Audio is never stored; only the scrubbed
transcript. Consent text appears before the microphone is requested.

## 7. Open source

Repository under Carlos's GitHub, Apache-2.0, codename until the name is chosen. Public:
`intake-core`, `intake-server`, `widget/`, `console/`, `prompts/`, Docker compose for
self-hosting. Not public: site keys, the three sites' configs, anything naming Neo or an
employer. NOTICE text follows Veilkit's. Open-core line: the hosted queue, dedup across sites,
and analytics would be the paid layer later; none of that is in Phase 1.

## 8. Schedule (fits the launch plan)

| Launch-plan day | Date | Intake work (Claude unless noted) |
|---|---|---|
| Day 3 | Sun Oct 26 | Repo, `intake-core` state machine + triage schema with tests, Postgres schema |
| Day 4 | Mon Oct 27 | `intake-server` public API, scrub, persona turn against a fake model; rate limits |
| Day 5 | Tue Oct 28 | Triage + dedup; console queue read-only |
| Day 6 | Wed Oct 29 | Console actions, GitHub OAuth + issue bridge; widget v1 with Veilkit scrub |
| Day 7 | Thu Oct 30 | Deploy to Fly.io + Neon; embed on `morningqueue.com` staging; Carlos sees the queue |
| Day 8 | Fri Oct 31 | Prompt tuning with real partner conversations; teaching doc (Rust particulars) |
| Day 9 | Sat Nov 1 | Embed on `app.morningqueue.com`; copy review |
| Day 13 | Wed Nov 5 | In the dress rehearsal: file one bug and one feature through the widget end to end |
| After launch | | Code Warrior and Neo embeds; voice; name and public repo announcement |

**Cost to the launch plan:** these are Claude's hours, not the founder's, but they displace
Morning Queue polish in the same days. Carlos's only tasks: approve the Fly.io app and Neon
database on the entity's accounts (money, small), and the GitHub OAuth app registration.

## 9. Risks

- **Scope pressure on launch week.** Mitigation: the widget is additive; if Day 7 slips, launch
  without it and embed the week after. Nothing in Morning Queue depends on it.
- **Prompt injection.** Mitigated by design (§5.3); tested with a corpus of hostile inputs.
- **Privacy posture on Neo.** Scrubbing is not the same as a BAA; the Neo embed waits for
  counsel's view and is not in Phase 1.
- **Spam and abuse.** Rate limits, caps, optional Turnstile, and the queue is private.
- **Model cost.** Two to five small-model calls per conversation; negligible at launch volumes.

## 10. Success criteria for Phase 1

Widget live on both Morning Queue hosts by Day 9; a design partner's bug reaches a GitHub issue
through the queue with no manual retyping; zero raw identifiers in the database (verified by a
scan with Veilkit's CLI over a dump); p95 persona reply under 3 s; `intake-core` at 100% branch
coverage on the state machine.

## 11. Decisions requested from Carlos

1. Approve this architecture (the Phase 1 gate).
2. The working codename `intake` is fine until launch; name and public repo after.
3. Accounts: Fly.io app and Neon database under the entity, GitHub OAuth app on his account.
