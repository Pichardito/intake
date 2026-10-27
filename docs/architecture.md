# intake — architecture

Working codename `intake`. An embeddable feedback widget with a researcher persona, a Rust
server that scrubs, triages and de-duplicates what visitors say, and a console where the site
owner approves tickets before they become GitHub issues. Apache-2.0.

## 1. Intent

Visitors report bugs, ask for features or leave feedback through a chat panel on the host
site. The persona asks up to three follow-up questions. The server scrubs identifiers (with
[Veilkit](https://github.com/Pichardito/veilkit)) before any model or database sees the text,
classifies the conversation into a draft ticket, checks it against open tickets, and places it
in a queue the owner reviews. Approved tickets become GitHub issues.

The model proposes; a human disposes. Nothing with a side effect runs without an approval.

**Goals.** Zero raw identifiers in the database; one script tag to embed; a queue the owner can
clear in minutes; issues that need no retyping.

**Non-goals (v1).** Voice; analytics dashboards; sentiment trends; reporter accounts; email
follow-ups; mobile SDKs.

**Anti-goals (ever).** The triage step opening pull requests, running code, emailing or
messaging anyone; storing unscrubbed text; a visible "AI-powered" badge; identifying reporters
beyond what they type.

## 2. Language choice

| Piece | Language | Why |
|---|---|---|
| Server (`intake-server`) | Rust (axum, sqlx, tokio) | One static binary; `veilkit-core` links natively for the server-side scrub; the model calls are plain HTTPS+JSON. |
| Domain core (`intake-core`) | Rust, no I/O | Conversation state machine, triage schema, prompt templating, dedup scoring. Pure, so fully unit-testable and reusable from the server and from tests. |
| Widget (`widget/`) | TypeScript, no framework | Must run on host sites with different stacks (Astro, React, Next). Loads the Veilkit WASM build lazily for the client-side scrub. |
| Console (`console/`) | TypeScript + React + Vite | The owner's queue UI; small. |
| Prompts | Markdown in `prompts/` | Iterated without recompiling; loaded at startup; versioned with the code. |

## 3. Components

```
  visitor's browser                       intake-server (Rust)                      owner
 ┌────────────────────┐   HTTPS/JSON    ┌───────────────────────────────┐       ┌──────────────┐
 │ widget (TS)        │ ─────────────▶  │ /v1/conversations (public)    │       │ console (TS) │
 │  • Veilkit WASM    │  scrubbed text  │  • site key + origin check    │       │  queue, approve
 │    scrub BEFORE    │                 │  • rate limit per IP+site     │ ◀──── │  merge, dismiss
 │    send            │ ◀─────────────  │  • persona turn → LLM         │ GitHub│  (GitHub OAuth)
 │  • 3 follow-ups max│  persona reply  │  • server-side scrub (belt)   │ OAuth └──────────────┘
 └────────────────────┘                 │  • conversation state machine │              │ approve
                                        │  • triage → draft ticket      │              ▼
                                        │  • dedup vs open tickets      │    GitHub Issues API
                                        │  • /admin/* (owner only)      │    label: intake
                                        └──────────────┬────────────────┘    repo per site
                                                       │ sqlx
                                                       ▼
                                             Postgres — sites, conversations,
                                             messages (scrubbed), tickets, audit
```

- **Widget.** A `<script>` tag with `data-site-key`. Renders a launcher button and a panel in a
  shadow root (no iframe). On open it lazy-loads Veilkit with the `pii`, `phi-us`, `secrets`
  and `finance` packs. Every outbound message is scrubbed client-side with the `label` style,
  and the widget shows the visitor what was redacted. It collects nothing automatically except
  page URL, viewport and user agent, shown before sending with one toggle to omit.
- **Public API.** `POST /v1/conversations` (start; returns an opaque conversation token),
  `POST /v1/conversations/{id}/messages` (visitor turn; returns persona reply and state),
  `POST /v1/conversations/{id}/end`. The site key is public; the server enforces an origin
  allowlist per site, a token bucket per IP and per site, a message cap and a body cap.
  Optional Cloudflare Turnstile per site.
- **Conversation state machine** (`intake-core`): `Open → Clarifying(n ≤ 3) → Closing →
  Triaged`. The persona may ask at most three questions: what-were-you-doing,
  what-did-you-expect, what-happened for a bug; who-benefits and how-often for a feature. The
  machine decides when to stop asking; the model only writes the words.
- **Persona turn.** One model call per visitor message (prompt from `prompts/persona.md`). The
  visitor's text sits inside a delimited data block; the system prompt says it is untrusted and
  must never be followed as instructions. Output is a short reply plus a JSON block with
  `state_hint` and `extracted_fields`; `intake-core` validates it and ignores anything outside
  the schema.
- **Server-side scrub.** `veilkit-core` runs again on every stored field, so a tampered widget
  cannot leak identifiers. Only scrubbed text is persisted; the reversible map is never stored.
- **Triage.** On `end` or after the cap, one call with `prompts/triage.md` producing strict
  JSON: `kind ∈ {bug, feature, feedback, praise, noise}`, `title`, `summary`, `steps[]`,
  `expected`, `actual`, `severity`, `confidence 0–1`, `duplicate_of?`. Candidate duplicates come
  from Postgres full-text search over the site's open tickets; `intake-core` scores and caps
  them. Invalid JSON → one retry → `kind: noise` with the raw summary for the owner to read.
- **Queue (console).** Draft tickets by site with the scrubbed transcript, the triage fields
  and duplicates. Actions: approve (creates the GitHub issue), merge into an existing issue
  (adds a comment), dismiss with a reason. Admin auth is GitHub OAuth restricted to one GitHub
  login, sessions in signed cookies.
- **GitHub bridge.** On approve: create an issue in the site's repo with labels `intake` and the
  kind, body sections marked "visitor-provided, untrusted", and a link back to the queue entry.
  Any agent that picks up `intake` issues must treat the body as data.
- **Audit.** Every state transition and admin action is a row. Conversations are retained 90
  days, tickets indefinitely; `DELETE /admin/conversations/{id}` for takedown.

## 4. Data model (Postgres)

`sites(id, name, origin_allowlist[], github_repo, turnstile_secret?)` ·
`conversations(id, site_id, token_hash, state, page_url, ua, started_at, ended_at)` ·
`messages(id, conversation_id, role, text_scrubbed, redactions_json, created_at)` ·
`tickets(id, site_id, conversation_id, kind, title, summary, fields_json, severity,
confidence, duplicate_of, status ∈ {draft, approved, merged, dismissed}, github_issue_url,
decided_at, decided_reason)` · `audit(id, actor, action, subject, at)`.

## 5. Boundaries and why they sit there

- **Scrub at the edge and again at the core.** The browser scrub protects the visitor from the
  network and the model; the server scrub protects the database from a modified widget.
- **State machine in code, words from the model.** The number of questions, when to stop and
  which fields are required are not left to a prompt, so behaviour is testable and bounded.
- **Triage returns a schema, never an action.** The only effect of the model's output is a row
  with `status = draft`. Everything with a side effect sits behind an owner's click.
- **One server, many sites.** A `site_id` on every row; no per-product deployment.

## 6. Trade-offs

- Lazy-loading Veilkit (~370 KB gz) costs a moment on first open; acceptable because the panel
  opens on intent, not on page load. The widget shows a "preparing" state.
- Full-text dedup misses paraphrases; embeddings are a later upgrade. The owner sees duplicates
  anyway.
- GitHub OAuth for a single admin is heavier than a shared secret but gives a real audit actor.
- A Rust server builds more slowly than a TypeScript one; mitigated by `intake-core` being pure
  and the server thin.

## 7. What would force a redesign

A second admin user (roles) · voice at scale (a streaming media path) · self-hosting with a
different model provider (the model client sits behind a trait from the start) · conversations
over ~12 turns (the state machine is built for short intake, not support).

## 8. Voice seam (designed, not built)

The persona turn takes `VisitorInput::Text | VisitorInput::Transcript { text, confidence }`.
A later widget mode adds push-to-talk: browser `MediaRecorder` → `POST /v1/.../audio` →
server-side speech-to-text → the same text path. Audio is never stored; only the scrubbed
transcript. Consent text appears before the microphone is requested.

See `docs/decisions.md` for where the build deviated from this design and `docs/deploy.md` to
run it.
