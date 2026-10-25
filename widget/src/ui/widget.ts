// Shadow-DOM shell: launcher + panel. Knows nothing about HTTP or Veilkit; it
// renders a ConversationSnapshot and reports user intent through callbacks.
import type { ConversationSnapshot, PageContext, Turn } from "../conversation";
import { MAX_TURNS } from "../conversation";
import { browserName } from "../context";
import { formatRedactions } from "../scrub/summary";
import { STYLES } from "./styles";

export interface WidgetOptions {
  host: HTMLElement;
  onSend: (text: string) => Promise<void>;
  onEnd: () => Promise<void>;
  /** Used only to describe, in plain words, what the context row will include. */
  describeContext?: () => PageContext;
}

export interface WidgetHandle {
  shadowRoot: ShadowRoot;
  render(snapshot: ConversationSnapshot): void;
  includeContext(): boolean;
  open(): void;
  close(): void;
  readonly stylesText: string;
}

const COPY = {
  launcher: "Feedback",
  title: "Send feedback",
  close: "Close",
  intro:
    "Found a bug, want something, or have a thought? Tell us in your own words. We may ask up to three short follow-up questions.",
  note: "Identifiers such as SSNs, email addresses and card numbers are removed on this page before anything is sent.",
  contextLead: "To help us reproduce, this report will include:",
  omit: "Don't include these",
  preparing: "Preparing the privacy filter…",
  sending: "Sending…",
  placeholder: "What happened?",
  send: "Send",
  finish: "Finish",
  done: "Thanks, we've got it. We read every one.",
  reference: "Reference",
  mismatch:
    "The server removed more identifiers than this page did. This page may be running an old version; reload it before sending anything else.",
  turn: (used: number) => `Turn ${used} of ${MAX_TURNS}`,
} as const;

const FOCUSABLE = 'button:not([disabled]), textarea:not([disabled]), input:not([disabled]), [href], [tabindex]:not([tabindex="-1"])';

/** Visible as far as our own inline `display:none` toggles go (jsdom has no layout). */
function isShown(node: HTMLElement, within: HTMLElement): boolean {
  for (let n: HTMLElement | null = node; n && n !== within; n = n.parentElement) {
    if (n.style.display === "none") return false;
  }
  return true;
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, string> = {},
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  if (text !== undefined) node.textContent = text;
  return node;
}

export function mountWidget(opts: WidgetOptions): WidgetHandle {
  const container = el("div", { id: "intake-widget" });
  const shadow = container.attachShadow({ mode: "open" });
  const style = el("style", {}, STYLES);
  const root = el("div", { class: "root" });
  shadow.append(style, root);
  opts.host.appendChild(container);

  const launcher = el("button", { class: "launcher", type: "button", "aria-expanded": "false", "aria-haspopup": "dialog" }, COPY.launcher);
  root.append(launcher);

  let panel: HTMLElement | null = null;
  let regions: Regions | null = null;
  let includeContext = true;
  let last: ConversationSnapshot = { status: "idle", turns: [], turnsUsed: 0 };

  interface Regions {
    body: HTMLElement;
    intro: HTMLElement;
    context: HTMLElement;
    messages: HTMLElement;
    status: HTMLElement;
    counter: HTMLElement;
    composer: HTMLElement;
    textarea: HTMLTextAreaElement;
    sendBtn: HTMLButtonElement;
    finishBtn: HTMLButtonElement;
    done: HTMLElement;
  }

  function buildPanel(): void {
    panel = el("div", { class: "panel", role: "dialog", "aria-modal": "true", "aria-label": COPY.title });

    const header = el("div", { class: "header" });
    const title = el("h2", {}, COPY.title);
    const closeBtn = el("button", { class: "icon-btn", type: "button", "aria-label": COPY.close }, "✕");
    closeBtn.addEventListener("click", close);
    header.append(title, closeBtn);

    const body = el("div", { class: "body" });
    const intro = el("div", { class: "intro" });
    intro.append(el("p", {}, COPY.intro), el("p", { class: "note" }, COPY.note));
    const context = buildContextRow();
    const messages = el("div", { class: "messages", "aria-live": "polite" });
    messages.style.display = "contents";
    const status = el("div", { class: "status" });
    const counter = el("div", { class: "counter" });
    body.append(intro, context, messages, status, counter);

    const composer = el("div", { class: "composer" });
    const label = el("label", { class: "sr-only", for: "intake-input" }, COPY.placeholder);
    const textarea = el("textarea", { id: "intake-input", placeholder: COPY.placeholder, rows: "3" });
    const actions = el("div", { class: "actions" });
    const finishBtn = el("button", { class: "secondary", type: "button" }, COPY.finish);
    const sendBtn = el("button", { class: "primary", type: "button" }, COPY.send);
    actions.append(finishBtn, sendBtn);
    composer.append(label, textarea, actions);

    const done = el("div", { class: "done" });

    panel.append(header, body, composer, done);
    panel.addEventListener("keydown", onKeydown);

    const submit = () => {
      const text = textarea.value;
      if (!text.trim()) return;
      textarea.value = "";
      void opts.onSend(text);
    };
    sendBtn.addEventListener("click", submit);
    textarea.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
        e.preventDefault();
        submit();
      }
    });
    finishBtn.addEventListener("click", () => void opts.onEnd());

    regions = { body, intro, context, messages, status, counter, composer, textarea, sendBtn, finishBtn, done };
    root.insertBefore(panel, launcher);
  }

  function buildContextRow(): HTMLElement {
    const row = el("div", { class: "context" });
    const lead = el("div", {}, COPY.contextLead);
    const list = el("ul", { style: "margin:4px 0 0 18px;padding:0" });
    const ctx = opts.describeContext?.();
    const items = ctx
      ? [`this page's address (${ctx.page_url})`, `your window size (${ctx.viewport.replace("x", "×")})`, `your browser (${browserName(ctx.ua)})`]
      : ["this page's address", "your window size", "your browser"];
    for (const item of items) list.append(el("li", {}, item));
    const label = el("label");
    const toggle = el("input", { type: "checkbox", class: "omit-context" });
    toggle.addEventListener("change", () => {
      includeContext = !toggle.checked;
    });
    label.append(toggle, document.createTextNode(COPY.omit));
    row.append(lead, list, label);
    return row;
  }

  function onKeydown(e: KeyboardEvent): void {
    const dialog = panel;
    if (!dialog) return;
    if (e.key === "Escape") {
      e.preventDefault();
      close();
      return;
    }
    if (e.key !== "Tab") return;
    const nodes = Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((n) => isShown(n, dialog));
    if (nodes.length === 0) return;
    const first = nodes[0]!;
    const lastNode = nodes[nodes.length - 1]!;
    const active = shadow.activeElement;
    if (e.shiftKey && (active === first || !dialog.contains(active))) {
      e.preventDefault();
      lastNode.focus();
    } else if (!e.shiftKey && active === lastNode) {
      e.preventDefault();
      first.focus();
    }
  }

  function open(): void {
    if (panel) return;
    buildPanel();
    launcher.setAttribute("aria-expanded", "true");
    render(last);
    focusInitial();
  }

  function focusInitial(): void {
    if (!panel || !regions) return;
    const target = regions.composer.isConnected ? regions.textarea : panel.querySelector<HTMLElement>(FOCUSABLE);
    target?.focus();
  }

  function close(): void {
    if (!panel) return;
    panel.remove();
    panel = null;
    regions = null;
    launcher.setAttribute("aria-expanded", "false");
    launcher.focus();
  }

  function renderTurn(turn: Turn): HTMLElement[] {
    const out: HTMLElement[] = [el("div", { class: `msg ${turn.role}` }, turn.text)];
    if (turn.role === "visitor") {
      const line = formatRedactions(turn.redactions ?? {});
      if (line) out.push(el("div", { class: "redacted" }, line));
      if (turn.mismatch) out.push(el("div", { class: "mismatch", role: "alert" }, COPY.mismatch));
    }
    return out;
  }

  function render(snapshot: ConversationSnapshot): void {
    last = snapshot;
    if (!panel || !regions) return;
    const r = regions;
    const started = snapshot.turns.length > 0;
    const isDone = snapshot.status === "done";

    if (started || isDone) {
      r.intro.remove();
      r.context.remove();
    }

    r.messages.replaceChildren(...snapshot.turns.flatMap(renderTurn));

    r.status.removeAttribute("role");
    if (snapshot.status === "preparing") r.status.textContent = COPY.preparing;
    else if (snapshot.status === "sending") r.status.textContent = COPY.sending;
    else if (snapshot.status === "error") {
      r.status.textContent = snapshot.error ?? "";
      r.status.setAttribute("role", "alert");
    } else r.status.textContent = "";

    r.counter.textContent = started && !isDone ? COPY.turn(snapshot.turnsUsed) : "";

    const busy = snapshot.status === "preparing" || snapshot.status === "sending";
    r.sendBtn.disabled = busy;
    r.textarea.disabled = busy;
    r.finishBtn.style.display = started ? "" : "none";
    r.finishBtn.disabled = busy;

    r.done.style.display = isDone ? "" : "none";
    if (isDone) {
      r.composer.remove();
      r.body.remove();
      const closeBtn = el("button", { class: "primary", type: "button" }, COPY.close);
      closeBtn.addEventListener("click", close);
      r.done.replaceChildren(
        el("p", {}, COPY.done),
        el("p", { class: "ref" }, `${COPY.reference}: ${snapshot.reference ?? ""}`),
        closeBtn,
      );
      if (!panel.contains(shadow.activeElement)) closeBtn.focus();
    }
    r.body.scrollTop = r.body.scrollHeight;
  }

  launcher.addEventListener("click", () => (panel ? close() : open()));

  return {
    shadowRoot: shadow,
    render,
    includeContext: () => includeContext,
    open,
    close,
    stylesText: STYLES,
  };
}
