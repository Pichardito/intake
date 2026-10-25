import { describe, expect, it, vi } from "vitest";
import { createApi } from "../src/api";
import { MAX_TURNS, createConversation, type ConversationSnapshot } from "../src/conversation";
import type { Scrubber } from "../src/scrub/types";

interface Recorded {
  url: string;
  method: string;
  headers: Record<string, string>;
  body: unknown;
}

/** A fake fetch that records every request and answers from a script. */
function fakeFetch(
  script: {
    message?: (n: number, body: { text: string }) => { reply: string; state: string; redactions: Record<string, number> };
    end?: () => { ticket_id: string | null; state: string };
    status?: number;
  } = {},
) {
  const calls: Recorded[] = [];
  let messageCount = 0;
  const fn: typeof fetch = async (input, init) => {
    const url = String(input);
    const body = init?.body ? (JSON.parse(String(init.body)) as unknown) : null;
    calls.push({
      url,
      method: init?.method ?? "GET",
      headers: Object.fromEntries(new Headers(init?.headers).entries()),
      body,
    });
    if (script.status && script.status >= 400) {
      return new Response(JSON.stringify({ error: "nope" }), { status: script.status });
    }
    if (url.endsWith("/v1/conversations")) {
      return Response.json({ id: "c_123", token: "tok_abc" });
    }
    if (url.endsWith("/messages")) {
      messageCount += 1;
      const reply = script.message?.(messageCount, body as { text: string }) ?? {
        reply: `reply ${messageCount}`,
        state: "clarifying",
        redactions: {},
      };
      return Response.json(reply);
    }
    if (url.endsWith("/end")) {
      return Response.json(script.end?.() ?? { ticket_id: "t_9", state: "triaged" });
    }
    return new Response("not found", { status: 404 });
  };
  return { fn, calls };
}

/** A scrubber that replaces digits-with-dashes by [SSN] and reports the count. */
const fakeScrubber: Scrubber = async (text) => {
  const matches = text.match(/\d{3}-\d{2}-\d{4}/g) ?? [];
  return {
    text: text.replace(/\d{3}-\d{2}-\d{4}/g, "[SSN]"),
    counts: matches.length ? { SSN: matches.length } : {},
  };
};

function setup(opts: {
  fetch?: ReturnType<typeof fakeFetch>;
  includeContext?: boolean;
  scrubber?: Scrubber;
} = {}) {
  const ff = opts.fetch ?? fakeFetch();
  const snapshots: ConversationSnapshot[] = [];
  const convo = createConversation({
    api: createApi("https://feedback.example.test", ff.fn),
    siteKey: "sk_test",
    loadScrubber: async () => opts.scrubber ?? fakeScrubber,
    context: () =>
      opts.includeContext === false
        ? null
        : { page_url: "https://example.test/p", ua: "UA/1", viewport: "800x600" },
    onChange: (s) => snapshots.push(s),
  });
  return { convo, ff, snapshots };
}

describe("conversation pipeline", () => {
  it("scrubs the text before it reaches fetch and records the redaction counts", async () => {
    const { convo, ff } = setup();
    await convo.send("my ssn is 078-05-1120");

    const msg = ff.calls.find((c) => c.url.endsWith("/messages"));
    expect(msg).toBeDefined();
    expect(msg?.body).toEqual({ text: "my ssn is [SSN]", source: "text" });
    expect(JSON.stringify(ff.calls)).not.toContain("078-05-1120");

    const snap = convo.snapshot();
    const visitor = snap.turns[0];
    expect(visitor?.role).toBe("visitor");
    expect(visitor?.text).toBe("my ssn is [SSN]");
    expect(visitor?.redactions).toEqual({ SSN: 1 });
    expect(visitor?.mismatch).toBe(false);
  });

  it("starts the conversation once, with the site key, the bearer token and the context", async () => {
    const { convo, ff } = setup();
    await convo.send("hello");
    await convo.send("again");

    const starts = ff.calls.filter((c) => c.url.endsWith("/v1/conversations"));
    expect(starts).toHaveLength(1);
    expect(starts[0]?.body).toEqual({
      site_key: "sk_test",
      page_url: "https://example.test/p",
      ua: "UA/1",
      viewport: "800x600",
    });
    const msgs = ff.calls.filter((c) => c.url.endsWith("/messages"));
    expect(msgs).toHaveLength(2);
    expect(msgs[0]?.url).toBe("https://feedback.example.test/v1/conversations/c_123/messages");
    expect(msgs[0]?.headers["authorization"]).toBe("Bearer tok_abc");
  });

  it("sends only the site key when the visitor opted out of context", async () => {
    const { convo, ff } = setup({ includeContext: false });
    await convo.send("hello");
    const start = ff.calls.find((c) => c.url.endsWith("/v1/conversations"));
    expect(start?.body).toEqual({ site_key: "sk_test" });
  });

  it("flags a mismatch when the server redacted more than the browser", async () => {
    const ff = fakeFetch({
      message: () => ({ reply: "ok", state: "clarifying", redactions: { SSN: 1, EMAIL: 1 } }),
    });
    const { convo } = setup({ fetch: ff });
    await convo.send("ssn 078-05-1120");
    expect(convo.snapshot().turns[0]?.mismatch).toBe(true);
  });

  it("appends the persona reply and counts visitor turns", async () => {
    const { convo } = setup();
    await convo.send("one");
    await convo.send("two");
    const snap = convo.snapshot();
    expect(snap.turnsUsed).toBe(2);
    expect(snap.turns.map((t) => t.role)).toEqual(["visitor", "assistant", "visitor", "assistant"]);
    expect(snap.turns[1]?.text).toBe("reply 1");
    expect(snap.status).toBe("chatting");
  });

  it("ends with a reference id when the server reaches closing", async () => {
    const ff = fakeFetch({
      message: (n) => ({ reply: n === 2 ? "Got it, thanks." : "Tell me more?", state: n === 2 ? "closing" : "clarifying", redactions: {} }),
      end: () => ({ ticket_id: "t_42", state: "triaged" }),
    });
    const { convo } = setup({ fetch: ff });
    await convo.send("one");
    expect(convo.snapshot().status).toBe("chatting");
    await convo.send("two");
    const snap = convo.snapshot();
    expect(snap.status).toBe("done");
    expect(snap.reference).toBe("t_42");
    expect(ff.calls.filter((c) => c.url.endsWith("/end"))).toHaveLength(1);
  });

  it("ends automatically after the 12th visitor turn", async () => {
    const ff = fakeFetch();
    const { convo } = setup({ fetch: ff });
    for (let i = 0; i < MAX_TURNS; i += 1) {
      await convo.send(`turn ${i + 1}`);
    }
    const snap = convo.snapshot();
    expect(snap.turnsUsed).toBe(12);
    expect(snap.status).toBe("done");
    expect(ff.calls.filter((c) => c.url.endsWith("/end"))).toHaveLength(1);
    await convo.send("thirteen");
    expect(ff.calls.filter((c) => c.url.endsWith("/messages"))).toHaveLength(12);
  });

  it("falls back to the conversation id as reference when there is no ticket yet", async () => {
    const ff = fakeFetch({ end: () => ({ ticket_id: null, state: "closing" }) });
    const { convo } = setup({ fetch: ff });
    await convo.send("hi");
    await convo.end();
    expect(convo.snapshot().status).toBe("done");
    expect(convo.snapshot().reference).toBe("c_123");
  });

  it("surfaces a generic error and keeps the draft when the server fails", async () => {
    const ff = fakeFetch({ status: 500 });
    const { convo, snapshots } = setup({ fetch: ff });
    await convo.send("hello");
    const snap = convo.snapshot();
    expect(snap.status).toBe("error");
    expect(snap.error).toMatch(/could not send/i);
    expect(snap.turns).toHaveLength(0);
    expect(snapshots.some((s) => s.status === "sending")).toBe(true);
  });

  it("reports a preparing state while the scrubber loads and never sends raw text if it fails", async () => {
    const ff = fakeFetch();
    const snapshots: ConversationSnapshot[] = [];
    const convo = createConversation({
      api: createApi("https://feedback.example.test", ff.fn),
      siteKey: "sk_test",
      loadScrubber: async () => {
        throw new Error("wasm failed");
      },
      context: () => null,
      onChange: (s) => snapshots.push(s),
    });
    await convo.send("ssn 078-05-1120");
    expect(snapshots.some((s) => s.status === "preparing")).toBe(true);
    expect(convo.snapshot().status).toBe("error");
    expect(ff.calls).toHaveLength(0);
  });

  it("ignores blank input", async () => {
    const { convo, ff } = setup();
    await convo.send("   ");
    expect(ff.calls).toHaveLength(0);
  });

  it("exposes the spy-friendly onChange with the latest snapshot", async () => {
    const onChange = vi.fn();
    const ff = fakeFetch();
    const convo = createConversation({
      api: createApi("https://feedback.example.test", ff.fn),
      siteKey: "sk",
      loadScrubber: async () => fakeScrubber,
      context: () => null,
      onChange,
    });
    await convo.send("x");
    expect(onChange).toHaveBeenCalled();
    expect(onChange.mock.lastCall?.[0]).toEqual(convo.snapshot());
  });
});
