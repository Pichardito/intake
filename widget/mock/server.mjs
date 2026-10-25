// Tiny in-memory mock of the intake-server API (docs/phase2-spec.md §3.3).
// Used by the widget and console tests and by `pnpm dev` in both packages
// until the Rust server exists. Node only, no dependencies.
//
//   node mock/server.mjs            # http://127.0.0.1:8787
//   PORT=9000 node mock/server.mjs
//
// Serves: /v1/* public API, /admin/* admin API (cookie "intake_session"),
// /widget/* from widget/dist, /console/* from console/dist, /demo host page.
import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { dirname, extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { seedTickets } from "./seed.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const WIDGET_DIST = join(here, "..", "dist");
const CONSOLE_DIST = join(here, "..", "..", "console", "dist");
const BODY_CAP = 4096;
const SESSION_COOKIE = "intake_session";

const MIME = {
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".wasm": "application/wasm",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".map": "application/json",
};

const SITES = [
  { id: 1, key: "sk_site_demo", name: "Morning Queue site", github_repo: "example/morningqueue-site" },
  { id: 2, key: "sk_app_demo", name: "Morning Queue app", github_repo: "example/morningqueue-app" },
];

const PERSONA_SCRIPT = [
  "Thanks. What were you doing right before this happened?",
  "What did you expect to happen?",
  "And what happened instead?",
];

/** Labels the "server" claims to redact: count of [LABEL] placeholders already in the text. */
function serverRedactions(text) {
  const counts = {};
  for (const m of text.matchAll(/\[([A-Z_]+)\]/g)) counts[m[1]] = (counts[m[1]] ?? 0) + 1;
  // Raw identifiers that reached the server mean the browser scrub was bypassed.
  const rawSsn = text.match(/\b\d{3}-\d{2}-\d{4}\b/g)?.length ?? 0;
  const rawEmail = text.match(/[\w.+-]+@[\w-]+\.[\w.]+/g)?.length ?? 0;
  if (rawSsn) counts.SSN = (counts.SSN ?? 0) + rawSsn;
  if (rawEmail) counts.EMAIL = (counts.EMAIL ?? 0) + rawEmail;
  // Test hook: the literal token MISMATCH makes the server report one more EMAIL.
  if (text.includes("MISMATCH")) counts.EMAIL = (counts.EMAIL ?? 0) + 1;
  return counts;
}

export function createMock(options = {}) {
  const log = options.log ?? (() => {});
  const conversations = new Map();
  const tickets = new Map();
  let nextTicketId = 1;
  for (const t of seedTickets()) {
    tickets.set(t.id, t);
    nextTicketId = Math.max(nextTicketId, t.id + 1);
  }
  const audit = [];

  const json = (res, status, body, extraHeaders = {}) => {
    const data = JSON.stringify(body);
    res.writeHead(status, {
      "content-type": "application/json; charset=utf-8",
      "access-control-allow-origin": "*",
      "access-control-allow-headers": "authorization, content-type",
      "access-control-allow-methods": "GET, POST, DELETE, OPTIONS",
      ...extraHeaders,
    });
    res.end(data);
  };

  const readBody = (req) =>
    new Promise((resolve, reject) => {
      let size = 0;
      const chunks = [];
      req.on("data", (c) => {
        size += c.length;
        if (size > BODY_CAP) {
          reject(Object.assign(new Error("body too large"), { status: 413 }));
          req.destroy();
          return;
        }
        chunks.push(c);
      });
      req.on("end", () => {
        if (chunks.length === 0) return resolve({});
        try {
          resolve(JSON.parse(Buffer.concat(chunks).toString("utf8")));
        } catch {
          reject(Object.assign(new Error("invalid json"), { status: 400 }));
        }
      });
      req.on("error", reject);
    });

  const bearer = (req) => {
    const h = req.headers.authorization ?? "";
    return h.startsWith("Bearer ") ? h.slice(7) : null;
  };

  const hasSession = (req) => {
    const cookie = req.headers.cookie ?? "";
    return cookie.split(";").some((c) => c.trim() === `${SESSION_COOKIE}=dev`);
  };

  async function serveStatic(res, root, rel, cacheImmutable = false) {
    const safe = normalize("/" + rel).replace(/^(\.\.[/\\])+/, "");
    let file = join(root, safe);
    try {
      const s = await stat(file);
      if (s.isDirectory()) file = join(file, "index.html");
    } catch {
      // SPA fallback for the console; 404 for everything else.
      if (root === CONSOLE_DIST) file = join(root, "index.html");
    }
    try {
      const data = await readFile(file);
      res.writeHead(200, {
        "content-type": MIME[extname(file)] ?? "application/octet-stream",
        "cache-control": cacheImmutable ? "public, max-age=31536000, immutable" : "no-cache",
        "access-control-allow-origin": "*",
      });
      res.end(data);
    } catch {
      res.writeHead(404, { "content-type": "text/plain" });
      res.end(`not found: ${rel} (did you run pnpm build?)`);
    }
  }

  function demoPage(port) {
    return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Host page (mock)</title><style>body{font:16px system-ui;margin:40px;max-width:640px}</style></head>
<body><h1>A host page</h1><p>This page stands in for a product site. The feedback launcher is bottom-right.</p>
<script src="http://127.0.0.1:${port}/widget/intake.js" data-site-key="sk_site_demo" defer></script>
</body></html>`;
  }

  // ---- public API --------------------------------------------------------

  function startConversation(body) {
    const site = SITES.find((s) => s.key === body.site_key);
    if (!site) return { status: 403, body: { error: "unknown site key" } };
    const id = `c_${randomUUID().slice(0, 8)}`;
    const token = `tok_${randomUUID()}`;
    conversations.set(id, {
      id,
      token,
      site_id: site.id,
      state: "open",
      page_url: body.page_url ?? null,
      ua: body.ua ?? null,
      viewport: body.viewport ?? null,
      messages: [],
      started_at: new Date().toISOString(),
      ended_at: null,
      ticket_id: null,
    });
    return { status: 200, body: { id, token } };
  }

  function visitorMessage(convo, body) {
    if (typeof body.text !== "string" || !body.text.trim()) return { status: 400, body: { error: "text required" } };
    if (convo.state === "triaged") return { status: 409, body: { error: "conversation ended" } };
    const visitorTurns = convo.messages.filter((m) => m.role === "visitor").length;
    if (visitorTurns >= 12) return { status: 409, body: { error: "turn cap reached" } };
    const redactions = serverRedactions(body.text);
    const now = new Date().toISOString();
    convo.messages.push({ role: "visitor", text: body.text, redactions, created_at: now });
    const asked = visitorTurns; // follow-ups asked so far
    let reply;
    if (asked < PERSONA_SCRIPT.length) {
      reply = PERSONA_SCRIPT[asked];
      convo.state = "clarifying";
    } else {
      reply = "Got it. That is everything I need; thank you.";
      convo.state = "closing";
    }
    convo.messages.push({ role: "assistant", text: reply, redactions: {}, created_at: now });
    return { status: 200, body: { reply, state: convo.state, redactions } };
  }

  function endConversation(convo) {
    if (convo.ticket_id) return { status: 200, body: { ticket_id: String(convo.ticket_id), state: convo.state } };
    const visitor = convo.messages.filter((m) => m.role === "visitor").map((m) => m.text);
    if (visitor.length === 0) {
      convo.state = "triaged";
      convo.ended_at = new Date().toISOString();
      return { status: 200, body: { ticket_id: null, state: convo.state } };
    }
    const site = SITES.find((s) => s.id === convo.site_id);
    const id = nextTicketId++;
    const first = visitor[0];
    const ticket = {
      id,
      site_id: site.id,
      site: site.name,
      conversation_id: convo.id,
      kind: /should|could|would be nice|feature|add /i.test(first) ? "feature" : "bug",
      title: first.length > 60 ? `${first.slice(0, 57)}…` : first,
      summary: visitor.join(" "),
      steps: visitor.slice(1, 2),
      expected: visitor[2] ?? null,
      actual: visitor[3] ?? null,
      severity: "medium",
      confidence: 0.55,
      duplicate_of: null,
      status: "draft",
      github_issue_url: null,
      created_at: new Date().toISOString(),
      decided_at: null,
      decided_reason: null,
      page_url: convo.page_url,
      ua: convo.ua,
      viewport: convo.viewport,
      transcript: convo.messages.map((m) => ({ ...m })),
    };
    tickets.set(id, ticket);
    convo.ticket_id = id;
    convo.state = "triaged";
    convo.ended_at = new Date().toISOString();
    audit.push({ actor: "system", action: "triaged", subject: `ticket:${id}`, at: ticket.created_at });
    return { status: 200, body: { ticket_id: String(id), state: convo.state } };
  }

  // ---- admin API ---------------------------------------------------------

  const publicTicket = (t) => Object.fromEntries(Object.entries(t).filter(([k]) => k !== "transcript"));

  function duplicatesFor(t) {
    const words = (s) => new Set((s ?? "").toLowerCase().match(/[a-z0-9]{3,}/g) ?? []);
    const a = words(`${t.title} ${t.summary}`);
    return [...tickets.values()]
      .filter((o) => o.id !== t.id && o.site_id === t.site_id && o.status !== "dismissed")
      .map((o) => {
        const b = words(`${o.title} ${o.summary}`);
        const inter = [...a].filter((w) => b.has(w)).length;
        const union = new Set([...a, ...b]).size || 1;
        return { id: o.id, title: o.title, status: o.status, github_issue_url: o.github_issue_url, score: inter / union };
      })
      .filter((d) => d.score >= 0.15 || d.id === t.duplicate_of)
      .sort((x, y) => y.score - x.score)
      .slice(0, 5);
  }

  function listTickets(url) {
    const site = url.searchParams.get("site");
    const status = url.searchParams.get("status");
    const kind = url.searchParams.get("kind");
    const minConf = url.searchParams.get("min_confidence");
    let rows = [...tickets.values()];
    if (site) rows = rows.filter((t) => String(t.site_id) === site);
    if (status) rows = rows.filter((t) => t.status === status);
    if (kind) rows = rows.filter((t) => t.kind === kind);
    if (minConf) rows = rows.filter((t) => t.confidence >= Number(minConf));
    rows.sort((a, b) => (a.created_at < b.created_at ? 1 : -1));
    return { tickets: rows.map(publicTicket), sites: SITES.map(({ id, name }) => ({ id, name })) };
  }

  function decide(t, status, patch, action) {
    Object.assign(t, patch, { status, decided_at: new Date().toISOString() });
    audit.push({ actor: "admin", action, subject: `ticket:${t.id}`, at: t.decided_at });
    log(`${action} ticket ${t.id}`);
  }

  // ---- router ------------------------------------------------------------

  const server = createServer(async (req, res) => {
    const url = new URL(req.url, `http://${req.headers.host ?? "localhost"}`);
    const path = url.pathname;
    const method = req.method ?? "GET";
    try {
      if (method === "OPTIONS") return json(res, 204, {});

      if (path === "/v1/health") return json(res, 200, { ok: true, mock: true });
      if (path === "/demo" || path === "/") {
        res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
        return res.end(demoPage(server.address().port));
      }
      if (path.startsWith("/widget/")) return serveStatic(res, WIDGET_DIST, path.slice("/widget/".length), true);
      if (path === "/console" || path.startsWith("/console/")) {
        return serveStatic(res, CONSOLE_DIST, path.slice("/console".length));
      }

      if (method === "POST" && path === "/v1/conversations") {
        const r = startConversation(await readBody(req));
        return json(res, r.status, r.body);
      }
      const convoMatch = path.match(/^\/v1\/conversations\/([^/]+)\/(messages|end)$/);
      if (method === "POST" && convoMatch) {
        const convo = conversations.get(decodeURIComponent(convoMatch[1]));
        if (!convo || convo.token !== bearer(req)) return json(res, 401, { error: "unauthorized" });
        const body = await readBody(req);
        const r = convoMatch[2] === "messages" ? visitorMessage(convo, body) : endConversation(convo);
        return json(res, r.status, r.body);
      }

      if (path === "/admin/login") {
        res.writeHead(302, {
          location: "/console/",
          "set-cookie": `${SESSION_COOKIE}=dev; Path=/; HttpOnly; SameSite=Lax`,
        });
        return res.end();
      }
      if (method === "POST" && path === "/admin/logout") {
        return json(res, 204, {}, { "set-cookie": `${SESSION_COOKIE}=; Path=/; Max-Age=0` });
      }
      if (path.startsWith("/admin/")) {
        if (!hasSession(req)) return json(res, 401, { error: "unauthorized" });
        if (method === "GET" && path === "/admin/tickets") return json(res, 200, listTickets(url));
        const m = path.match(/^\/admin\/tickets\/(\d+)(?:\/(approve|merge|dismiss))?$/);
        if (m) {
          const t = tickets.get(Number(m[1]));
          if (!t) return json(res, 404, { error: "not found" });
          if (method === "GET" && !m[2]) return json(res, 200, { ...t, duplicates: duplicatesFor(t) });
          if (method === "POST" && m[2]) {
            if (t.status !== "draft") return json(res, 409, { error: `ticket is ${t.status}` });
            const body = await readBody(req);
            if (m[2] === "approve") {
              const fields = body.fields ?? {};
              const site = SITES.find((s) => s.id === t.site_id);
              const issueNo = 100 + t.id;
              decide(t, "approved", { ...fields, github_issue_url: `https://github.com/${site.github_repo}/issues/${issueNo}` }, "approve");
              return json(res, 200, { ticket: publicTicket(t) });
            }
            if (m[2] === "merge") {
              const into = Number(body.into);
              if (!tickets.has(into)) return json(res, 400, { error: "unknown target" });
              decide(t, "merged", { duplicate_of: into }, "merge");
              return json(res, 200, { ticket: publicTicket(t) });
            }
            if (m[2] === "dismiss") {
              if (!body.reason) return json(res, 400, { error: "reason required" });
              decide(t, "dismissed", { decided_reason: body.note ? `${body.reason}: ${body.note}` : body.reason }, "dismiss");
              return json(res, 200, { ticket: publicTicket(t) });
            }
          }
        }
        const del = path.match(/^\/admin\/conversations\/([^/]+)$/);
        if (method === "DELETE" && del) {
          conversations.delete(decodeURIComponent(del[1]));
          return json(res, 204, {});
        }
        return json(res, 404, { error: "not found" });
      }
      return json(res, 404, { error: "not found" });
    } catch (err) {
      return json(res, err.status ?? 500, { error: err.message ?? "error" });
    }
  });

  return {
    server,
    tickets,
    conversations,
    audit,
    listen(port = 0, host = "127.0.0.1") {
      return new Promise((resolve) => {
        server.listen(port, host, () => resolve(`http://${host}:${server.address().port}`));
      });
    },
    close() {
      return new Promise((resolve) => server.close(() => resolve()));
    },
  };
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1];
if (isMain) {
  const mock = createMock({ log: (m) => console.log(`[mock] ${m}`) });
  const base = await mock.listen(Number(process.env.PORT ?? 8787));
  console.log(`[mock] intake API on ${base}  demo: ${base}/demo  console: ${base}/admin/login`);
}
