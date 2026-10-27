# Decisions log

Calls made while building, in the order they came up. Each is reversible unless marked. The
design (`docs/architecture.md`) stays the reference; this file records where the build deviated or
had to choose.

## 2026-10-25 — Rust core and server (PR: feat/rust-core-server)

1. **Current crate majors, not the spec's.** axum 0.8 (spec said 0.7; path params are
   `/{id}`), sqlx 0.9 (matches the installed `sqlx-cli` 0.9 so `.sqlx/` offline data
   round-trips), reqwest 0.13 (rustls by default). Pinning to older majors would have meant
   fighting yanked transitive versions for no benefit.
2. **Veilkit pinned to commit `715d122c`** of `github.com/Pichardito/veilkit` (its `main` at
   build time). Bump deliberately; the scrub test asserts on the `SSN` label.
3. **`Open → Closing` directly** when the model says `enough` *and* nothing is required (praise,
   noise, or a first message that already carries every bug field). The spec table only listed
   `Open → Clarifying{1}`; forcing a question on "love it, thanks" would be a worse product.
   The cap (`asked ≤ 3`) is unchanged and property-tested.
4. **The state hint is advisory only inside `Clarifying`.** Completeness or the cap decide; the
   model's `ask_more`/`enough` can neither extend past three nor end early while a required
   field is missing. The hint is read only on the first turn (decision 3).
5. **Dedup resolution is explicit** (`resolve_duplicate`): a `duplicate_of` the model names
   that is not among the FTS candidates is dropped (anti-hallucination); the best candidate
   with Jaccard ≥ 0.35 becomes `duplicate_of`; the model's own suggestion earns a bounded +0.2
   nudge; when a duplicate is set, confidence is capped at 0.5 so a probable duplicate never
   looks like a confident new ticket. Thresholds are constants to tune after real data.
6. **FTS query is an OR of visitor tokens** (`to_tsquery('english', 'a | b | …')`, up to 24
   tokens from `intake_core::dedup::tokens`) rather than `plainto_tsquery`, which ANDs terms
   and finds nothing for a multi-sentence report. Candidates are the top 20 by `ts_rank`
   among `draft`/`approved` tickets of the same site.
7. **Persona parse failure twice → fixed fallback line**, the visitor's message is still
   stored, the state does not move, and nothing the model said is used. A visitor should not
   see "something went wrong" because the model fumbled the format.
8. **Triage parse failure twice → `Noise`, confidence 0, raw output as summary** (spec §2.4),
   recorded in the audit row as `fallback: true`. The hostile-input corpus exercises exactly
   this path and asserts GitHub is never called.
9. **Hand-rolled GitHub OAuth and signed cookies** instead of the `oauth2` and `axum-extra`
   crates: ~80 lines, both base URLs configurable so the integration test stubs GitHub with
   `wiremock`, and the HMAC cookie is a teaching point. Session = signed JSON `{login, exp}`,
   8 h, `HttpOnly; SameSite=Lax`, `Secure` when `PUBLIC_ORIGIN` is https. Nothing server-side,
   so logout is cookie-clearing only (acceptable for one admin).
10. **Approve flips status before calling GitHub**, then stores the URL. A double click gets a
    409 instead of two issues; a GitHub failure leaves an `approved` ticket with no URL, which
    the console should show as "retry" (not yet built — see Left out).
11. **Merge target is a ticket id**, not a raw issue number: the target must be in the same
    site and already carry a `github_issue_url`. The merged ticket records `duplicate_of` and
    the target's issue URL.
12. **Takedown keeps the ticket, drops the link.** `DELETE /admin/conversations/{id}` cascades
    messages; `tickets.conversation_id` becomes NULL. The ticket text is already scrubbed and
    may already be a public GitHub issue, so deleting it would not achieve anything.
13. **Context fields are scrubbed and clipped** (page URL, UA, viewport → Veilkit, 512 chars).
    A URL can carry an email in a path.
14. **Empty origin allowlist = no origin check** for a site (self-host convenience). Every
    production site must list its origins; `add-site` prints what it stored.
15. **Rate limits are in-process** (`governor`, 10/min keyed by client IP + site key for
    starts, + conversation id for messages). One Fly machine, so no shared store. Client IP
    comes from `fly-client-ip` / `x-forwarded-for` only when `TRUST_PROXY=true`.
16. **Hard cap of 12 visitor messages** per conversation triggers triage even if the state
    machine has not closed (it always closes by the 4th message; the cap is for a broken client).
17. **Local tests ran on a Homebrew `postgresql@16` scratch cluster on :5433**, not the
    Podman/Docker Postgres: the Podman VM was unreachable (SSH handshake reset) and restarting
    it would have interrupted an unrelated project's dev stack. `sqlx::test` creates a database
    per test. `.sqlx/` was generated against that cluster.
18. **Model default `claude-haiku-5-5`** (from `.env.example`); the request sends only `model`,
    `max_tokens`, `system`, `messages` with `anthropic-version: 2023-06-01`. No thinking or
    effort parameters, so the same body works across model generations. Timeout 20 s.
19. **Prompt versions** are a `<!-- version: YYYY-MM-DD.n -->` first line, exposed on
    `/v1/health` and written into every `ticket.drafted` audit row.
20. **A fallback triage never gets a duplicate link.** Found by the hostile corpus: the fallback
    summary is the raw model output, identical across failures, so by the third unparseable
    reply FTS + Jaccard marked it a duplicate of the previous one. `resolve_duplicate` now runs
    only on a parsed triage; fallbacks stay `duplicate_of = NULL`, confidence 0.
21. **Rate-limit test harness rotates client addresses** (`start_from(ip)`), because the
    12-case corpus from one address correctly tripped the 10/min start bucket. The limiter was
    right; the test was the client.

22. **API contract aligned with the widget/console PR (#1).** Public `state` is the bare label
    `open|clarifying|closing|triaged` with the question count as a sibling `asked`; message
    responses also carry `done` and `ticket_id` (the widget's reference id is
    `ticket_id ?? conversation id`). `GET /admin/tickets` returns `{tickets, sites}`;
    `GET /admin/tickets/{id}` returns `transcript[]` and `duplicates[] {id, title, status,
    github_issue_url, score}` (FTS over the ticket's own words, scored by the same Jaccard the
    triage uses, the recorded `duplicate_of` always included); `POST …/approve {fields}` takes
    the owner's edited triage fields (`deny_unknown_fields`), persists them, and builds the
    issue from the edited values. `/admin/logout` is POST. The server auto-triages when the
    state machine reaches `closing`, so the widget's `end` call on `closing|triaged` is
    idempotent and returns the same ticket id.

### Left out of this PR (and why)

- **Retention job** (90-day conversation purge): needs a decision on where it runs (Fly cron
  machine vs. `pg_cron` on Neon). One `DELETE … where started_at < now() - interval '90 days'`
  once decided.
- **Approve retry after GitHub failure**: the data model supports it (approved, no URL); the
  endpoint and console affordance are a follow-up.
- **Turnstile**: column exists, verification not wired. Rate limits and the private queue
  cover launch volumes.
- **Voice seam**: `InputSource::Transcript` is accepted and stored like text; no audio route.
- **Metrics/tracing export**: `tracing` to stdout only.
