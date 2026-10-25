# Deploying intake (Fly.io + Neon)

The server is one static binary that needs a Postgres URL and a handful of secrets. These are
the exact commands; account creation and billing are the owner's steps and are not automated.

## 0. Prerequisites

- `flyctl` (`brew install flyctl`), logged in to the entity's Fly.io organization.
- A Neon project with a database named `intake` (create it in the Neon console; copy the
  pooled connection string **with** `?sslmode=require`).
- A GitHub OAuth App (Settings → Developer settings → OAuth Apps): homepage
  `https://feedback.morningqueue.com`, callback `https://feedback.morningqueue.com/admin/callback`.
- A fine-grained GitHub token with **Issues: read and write** on the repositories that receive
  tickets (one token, scoped to those repos only).

## 1. Create the app (once)

```bash
cd ~/project/intake
fly apps create intake --org <entity-org>
fly ips allocate-v4 --shared -a intake
fly ips allocate-v6 -a intake
```

`fly.toml` (commit it; no secrets inside):

```toml
app = "intake"
primary_region = "iad"

[build]
  dockerfile = "docker/Dockerfile"

[env]
  PORT = "8788"
  TRUST_PROXY = "true"           # Fly sets fly-client-ip; the rate limiter keys on it
  PUBLIC_ORIGIN = "https://feedback.morningqueue.com"
  INTAKE_MODEL = "claude-haiku-5-5"

[http_service]
  internal_port = 8788
  force_https = true
  auto_stop_machines = "off"     # spec §6: one machine, always on
  auto_start_machines = true
  min_machines_running = 1

  [[http_service.checks]]
    interval = "30s"
    timeout = "5s"
    method = "GET"
    path = "/v1/health"

[[vm]]
  size = "shared-cpu-1x"
  memory = "512mb"
```

## 2. Secrets

```bash
fly secrets set -a intake \
  DATABASE_URL='postgres://<user>:<pw>@<neon-host>/intake?sslmode=require' \
  ANTHROPIC_API_KEY='sk-ant-…' \
  GITHUB_CLIENT_ID='…' \
  GITHUB_CLIENT_SECRET='…' \
  GITHUB_TOKEN='github_pat_…' \
  ADMIN_GITHUB_LOGIN='<your-github-login>' \
  SESSION_SECRET="$(openssl rand -base64 48)"
```

The server refuses to start if any of these is missing (`Config::from_env`), so a typo shows up
in `fly logs` within seconds, not on the first request.

## 3. Deploy and migrate

Migrations run at boot (`MIGRATOR.run` in `main.rs`), so the first deploy creates the schema.

```bash
fly deploy -a intake
fly logs -a intake          # expect "intake-server starting" then "listening"
curl -s https://intake.fly.dev/v1/health
```

## 4. Custom domain

```bash
fly certs add feedback.morningqueue.com -a intake
# then at the DNS provider: CNAME feedback → intake.fly.dev
fly certs show feedback.morningqueue.com -a intake
```

Later sites CNAME their own subdomain to the same app; the server is multi-site and the
`PUBLIC_ORIGIN` only matters for the admin callback and the queue links in issues.

## 5. Register a site

```bash
fly ssh console -a intake -C "/app/intake-server admin add-site \
  --key mq-site --name 'Morning Queue' \
  --blurb 'a collections desk for bookkeepers' \
  --origin https://morningqueue.com --origin https://www.morningqueue.com \
  --repo <owner>/<repo>"
fly ssh console -a intake -C "/app/intake-server admin list-sites"
```

The CORS allowlist refreshes every 60 s; no restart needed. The widget tag is then:

```html
<script src="https://feedback.morningqueue.com/widget/intake.js" data-site-key="mq-site" defer></script>
```

## 6. Operate

- Queue: `https://feedback.morningqueue.com/console` (GitHub login; only `ADMIN_GITHUB_LOGIN` passes).
- Logs: `fly logs -a intake`. Prompt versions appear on `/v1/health` and in every triage audit row.
- Takedown: `DELETE /admin/conversations/{id}` from the console or `curl` with the session cookie.
- Retention (90 days, architecture §5.1) is **not yet automated** — see `docs/decisions.md`.
- Rollback: `fly releases -a intake` then `fly deploy --image <previous image>`.

## Local self-hosting

```bash
cp .env.example .env   # fill the secrets
docker compose -f docker/compose.yml up --build
docker compose -f docker/compose.yml run --rm server admin add-site --key demo --name Demo --repo owner/repo
```
