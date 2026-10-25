import { useEffect } from "react";

export type Key = "j" | "k" | "a" | "m" | "d" | "Enter";
export type KeyHandlers = Partial<Record<Key, () => void>>;

function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable;
}

/**
 * Single-key shortcuts (j/k move, a approve, m merge, d dismiss, Enter open).
 * Ignored while typing in a field, while a dialog is open, or with modifiers.
 */
export function useKeys(handlers: KeyHandlers, enabled = true): void {
  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (isEditable(e.target)) return;
      if (document.querySelector("[role=dialog]")) return;
      const fn = handlers[e.key as Key];
      if (fn) {
        e.preventDefault();
        fn();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [handlers, enabled]);
}
