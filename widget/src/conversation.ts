// The message pipeline: scrub → send → reconcile counts → advance state.
// Pure of DOM; the UI subscribes through `onChange`.
import type { ConversationState, IntakeApi, StartRequest } from "./api";
import { serverRedactedMore } from "./scrub/summary";
import type { RedactionCounts, Scrubber } from "./scrub/types";

export const MAX_TURNS = 12;

export interface PageContext {
  page_url: string;
  ua: string;
  viewport: string;
}

export interface Turn {
  role: "visitor" | "assistant";
  text: string;
  /** Browser-side redaction counts (visitor turns only). */
  redactions?: RedactionCounts;
  /** True when the server redacted more than the browser did. */
  mismatch?: boolean;
}

export type Status = "idle" | "preparing" | "chatting" | "sending" | "done" | "error";

export interface ConversationSnapshot {
  status: Status;
  turns: Turn[];
  /** Visitor turns used so far, out of MAX_TURNS. */
  turnsUsed: number;
  reference?: string;
  error?: string;
}

export interface ConversationDeps {
  api: IntakeApi;
  siteKey: string;
  loadScrubber: () => Promise<Scrubber>;
  /** Page context to attach at start, or null when the visitor opted out. */
  context: () => PageContext | null;
  onChange: (snapshot: ConversationSnapshot) => void;
}

export interface Conversation {
  send(text: string): Promise<void>;
  end(): Promise<void>;
  snapshot(): ConversationSnapshot;
}

const GENERIC_ERROR = "Could not send. Please try again.";
const SCRUB_ERROR = "Could not prepare the privacy filter. Nothing was sent. Please try again.";

export function createConversation(deps: ConversationDeps): Conversation {
  let status: Status = "idle";
  const turns: Turn[] = [];
  let turnsUsed = 0;
  let reference: string | undefined;
  let error: string | undefined;
  let session: { id: string; token: string } | null = null;
  let scrubber: Scrubber | null = null;

  const snapshot = (): ConversationSnapshot => {
    const s: ConversationSnapshot = { status, turns: [...turns], turnsUsed };
    if (reference !== undefined) s.reference = reference;
    if (error !== undefined) s.error = error;
    return s;
  };

  const emit = (next: Status) => {
    status = next;
    deps.onChange(snapshot());
  };

  async function ensureScrubber(): Promise<Scrubber> {
    if (scrubber) return scrubber;
    emit("preparing");
    scrubber = await deps.loadScrubber();
    return scrubber;
  }

  async function ensureSession(): Promise<{ id: string; token: string }> {
    if (session) return session;
    const body: StartRequest = { site_key: deps.siteKey };
    const ctx = deps.context();
    if (ctx) Object.assign(body, ctx);
    session = await deps.api.start(body);
    return session;
  }

  async function finish(): Promise<void> {
    if (!session) return;
    const res = await deps.api.end(session.id, session.token);
    reference = res.ticket_id ?? session.id;
    emit("done");
  }

  const isTerminal = (state: ConversationState) => state === "closing" || state === "triaged";

  async function send(raw: string): Promise<void> {
    const text = raw.trim();
    if (!text || status === "done" || status === "sending" || status === "preparing") return;
    error = undefined;

    let scrub: Scrubber;
    try {
      scrub = await ensureScrubber();
    } catch {
      error = SCRUB_ERROR;
      emit("error");
      return;
    }

    emit("sending");
    try {
      const scrubbed = await scrub(text);
      const { id, token } = await ensureSession();
      const res = await deps.api.message(id, token, { text: scrubbed.text, source: "text" });
      turns.push({
        role: "visitor",
        text: scrubbed.text,
        redactions: scrubbed.counts,
        mismatch: serverRedactedMore(scrubbed.counts, res.redactions),
      });
      turns.push({ role: "assistant", text: res.reply });
      turnsUsed += 1;
      if (isTerminal(res.state) || turnsUsed >= MAX_TURNS) {
        await finish();
      } else {
        emit("chatting");
      }
    } catch {
      error = GENERIC_ERROR;
      emit("error");
    }
  }

  async function end(): Promise<void> {
    if (status === "done" || !session) return;
    try {
      await finish();
    } catch {
      error = GENERIC_ERROR;
      emit("error");
    }
  }

  return { send, end, snapshot };
}
