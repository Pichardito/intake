-- intake: initial schema. Every row carries a site_id; the server is multi-site.

create table sites (
    id               bigint generated always as identity primary key,
    key              text        not null unique,          -- public site key the widget sends
    name             text        not null,
    product_blurb    text        not null default '',       -- one-liner given to the persona
    origin_allowlist text[]      not null default '{}',     -- CORS + Origin check
    github_repo      text        not null,                  -- owner/repo for approved tickets
    turnstile_secret text,                                  -- optional Cloudflare Turnstile
    created_at       timestamptz not null default now()
);

create table conversations (
    id          uuid        primary key,
    site_id     bigint      not null references sites(id) on delete cascade,
    token_hash  text        not null unique,                -- argon2 of the bearer token
    state       text        not null default 'open',        -- open | clarifying:n | closing | triaged
    extracted   jsonb       not null default '{}'::jsonb,   -- intake_core::Extracted, merged per turn
    page_url    text,
    ua          text,
    viewport    text,
    started_at  timestamptz not null default now(),
    ended_at    timestamptz
);
create index conversations_site_started_idx on conversations (site_id, started_at desc);

create table messages (
    id              bigint generated always as identity primary key,
    conversation_id uuid        not null references conversations(id) on delete cascade,
    role            text        not null check (role in ('user', 'assistant')),
    text_scrubbed   text        not null,
    redactions_json jsonb       not null default '{}'::jsonb,  -- {"SSN": 1, "EMAIL": 2}
    created_at      timestamptz not null default now()
);
create index messages_conversation_idx on messages (conversation_id, id);

create table tickets (
    id               bigint generated always as identity primary key,
    site_id          bigint      not null references sites(id) on delete cascade,
    conversation_id  uuid        references conversations(id) on delete set null,
    kind             text        not null check (kind in ('bug', 'feature', 'feedback', 'praise', 'noise')),
    title            text        not null,
    summary          text        not null,
    fields_json      jsonb       not null default '{}'::jsonb,  -- steps, expected, actual, extracted
    severity         text        not null check (severity in ('low', 'medium', 'high')),
    confidence       real        not null check (confidence >= 0 and confidence <= 1),
    duplicate_of     bigint      references tickets(id) on delete set null,
    status           text        not null default 'draft'
                     check (status in ('draft', 'approved', 'merged', 'dismissed')),
    github_issue_url text,
    decided_at       timestamptz,
    decided_reason   text,
    created_at       timestamptz not null default now(),
    fts              tsvector    generated always as
                     (to_tsvector('english', coalesce(title, '') || ' ' || coalesce(summary, ''))) stored
);
create index tickets_fts_idx on tickets using gin (fts);
create index tickets_site_status_idx on tickets (site_id, status, created_at desc);

create table audit (
    id      bigint      generated always as identity primary key,
    actor   text        not null,   -- 'system' | 'visitor' | github login
    action  text        not null,   -- conversation.started, ticket.drafted, ticket.approved, ...
    subject text        not null,   -- 'conversation:<uuid>' | 'ticket:<id>'
    detail  jsonb       not null default '{}'::jsonb,
    at      timestamptz not null default now()
);
create index audit_subject_idx on audit (subject, at desc);
