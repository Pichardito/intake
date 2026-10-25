import { beforeEach, describe, expect, it } from "vitest";
import { mountWidget, type WidgetHandle } from "../src/ui/widget";
import type { ConversationSnapshot } from "../src/conversation";

function idle(): ConversationSnapshot {
  return { status: "idle", turns: [], turnsUsed: 0 };
}

describe("widget shell (shadow DOM, dialog, focus)", () => {
  let handle: WidgetHandle;
  let root: ShadowRoot;

  beforeEach(() => {
    document.body.innerHTML = "";
    handle = mountWidget({
      host: document.body,
      onSend: async () => {},
      onEnd: async () => {},
    });
    handle.render(idle());
    root = handle.shadowRoot;
  });

  it("renders a launcher button and no panel until opened", () => {
    const launcher = root.querySelector<HTMLButtonElement>("button.launcher");
    expect(launcher).not.toBeNull();
    expect(launcher?.getAttribute("aria-expanded")).toBe("false");
    expect(root.querySelector("[role=dialog]")).toBeNull();
  });

  it("opens a dialog, moves focus inside, and Escape closes and restores focus", () => {
    const launcher = root.querySelector<HTMLButtonElement>("button.launcher")!;
    launcher.focus();
    launcher.click();

    const dialog = root.querySelector<HTMLElement>("[role=dialog]");
    expect(dialog).not.toBeNull();
    expect(dialog?.getAttribute("aria-modal")).toBe("true");
    expect(dialog?.getAttribute("aria-label")).toBeTruthy();
    expect(launcher.getAttribute("aria-expanded")).toBe("true");
    expect(root.activeElement).not.toBeNull();
    expect(dialog?.contains(root.activeElement)).toBe(true);

    dialog?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(root.querySelector("[role=dialog]")).toBeNull();
    expect(root.activeElement).toBe(launcher);
  });

  it("traps Tab inside the dialog", () => {
    root.querySelector<HTMLButtonElement>("button.launcher")!.click();
    const dialog = root.querySelector<HTMLElement>("[role=dialog]")!;
    const focusables = dialog.querySelectorAll<HTMLElement>(
      "button:not([disabled]), textarea:not([disabled]), input:not([disabled])",
    );
    const first = focusables[0]!;
    const last = focusables[focusables.length - 1]!;

    last.focus();
    const tab = new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true });
    dialog.dispatchEvent(tab);
    expect(tab.defaultPrevented).toBe(true);
    expect(root.activeElement).toBe(first);

    first.focus();
    const shiftTab = new KeyboardEvent("keydown", { key: "Tab", shiftKey: true, bubbles: true, cancelable: true });
    dialog.dispatchEvent(shiftTab);
    expect(shiftTab.defaultPrevented).toBe(true);
    expect(root.activeElement).toBe(last);
  });

  it("shows the context row with an omit toggle before the first send", () => {
    root.querySelector<HTMLButtonElement>("button.launcher")!.click();
    const toggle = root.querySelector<HTMLInputElement>("input.omit-context");
    expect(toggle).not.toBeNull();
    expect(handle.includeContext()).toBe(true);
    toggle!.click();
    expect(handle.includeContext()).toBe(false);
  });

  it("renders turns, the redaction line, the mismatch warning and the counter", () => {
    root.querySelector<HTMLButtonElement>("button.launcher")!.click();
    handle.render({
      status: "chatting",
      turnsUsed: 2,
      turns: [
        { role: "visitor", text: "my ssn is [SSN]", redactions: { SSN: 1 }, mismatch: false },
        { role: "assistant", text: "What were you doing?" },
        { role: "visitor", text: "email [EMAIL]", redactions: { EMAIL: 1 }, mismatch: true },
        { role: "assistant", text: "Thanks." },
      ],
    });
    const text = root.querySelector("[role=dialog]")!.textContent ?? "";
    expect(text).toContain("redacted: SSN ×1");
    expect(text).toContain("redacted: EMAIL ×1");
    expect(text).toContain("2 of 12");
    expect(root.querySelectorAll(".mismatch")).toHaveLength(1);
    // Context row disappears once the conversation has started.
    expect(root.querySelector("input.omit-context")).toBeNull();
  });

  it("renders the done state with the reference id and no composer", () => {
    root.querySelector<HTMLButtonElement>("button.launcher")!.click();
    handle.render({ status: "done", turns: [], turnsUsed: 3, reference: "t_42" });
    const dialog = root.querySelector("[role=dialog]")!;
    expect(dialog.textContent).toContain("Thanks, we've got it. We read every one.");
    expect(dialog.textContent).toContain("t_42");
    expect(dialog.querySelector("textarea")).toBeNull();
  });

  it("never says AI and carries no marketing copy", () => {
    root.querySelector<HTMLButtonElement>("button.launcher")!.click();
    handle.render({ status: "preparing", turns: [], turnsUsed: 0 });
    const all = (root.textContent ?? "") + (handle.stylesText ?? "");
    expect(all).not.toMatch(/\bAI\b/);
    expect(all).not.toMatch(/powered by/i);
  });
});
