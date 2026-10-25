// All styles live inside the shadow root; no external fonts, no host CSS leaks.
export const STYLES = `
:host { all: initial; }
*, *::before, *::after { box-sizing: border-box; }
.root {
  --bg: #ffffff; --fg: #1a1a1a; --muted: #5f6368; --line: #e3e3e3;
  --accent: #1f4ed8; --accent-fg: #ffffff; --bubble: #f1f3f4; --warn-bg: #fff4e5; --warn-fg: #7a3e00;
  --radius: 12px; --shadow: 0 8px 28px rgba(0,0,0,.18);
  font: 14px/1.45 system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  color: var(--fg);
  position: fixed; right: 16px; bottom: 16px; z-index: 2147483000;
  display: flex; flex-direction: column; align-items: flex-end; gap: 12px;
}
@media (prefers-color-scheme: dark) {
  .root {
    --bg: #1e1f22; --fg: #ececec; --muted: #a6a8ad; --line: #3a3b40;
    --accent: #7aa2ff; --accent-fg: #0b1430; --bubble: #2b2d31; --warn-bg: #3a2a10; --warn-fg: #ffd59a;
  }
}
button, textarea, input { font: inherit; color: inherit; }
button { cursor: pointer; }
button:focus-visible, textarea:focus-visible, input:focus-visible, a:focus-visible {
  outline: 2px solid var(--accent); outline-offset: 2px;
}
.launcher {
  border: 0; border-radius: 999px; padding: 10px 16px;
  background: var(--accent); color: var(--accent-fg); font-weight: 600; box-shadow: var(--shadow);
}
.panel {
  width: 360px; height: 520px; max-height: calc(100vh - 32px);
  background: var(--bg); border: 1px solid var(--line); border-radius: var(--radius);
  box-shadow: var(--shadow); display: flex; flex-direction: column; overflow: hidden;
  animation: rise .16s ease-out;
}
@keyframes rise { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
@media (prefers-reduced-motion: reduce) { .panel { animation: none; } }
@media (max-width: 480px) {
  .root { right: 0; bottom: 0; }
  .panel { width: 100vw; height: 100vh; height: 100dvh; max-height: none; border-radius: 0; border: 0; }
}
.header { display: flex; align-items: center; justify-content: space-between; padding: 12px 14px; border-bottom: 1px solid var(--line); }
.header h2 { margin: 0; font-size: 15px; font-weight: 600; }
.icon-btn { border: 0; background: transparent; padding: 6px; border-radius: 8px; line-height: 1; color: var(--muted); }
.icon-btn:hover { background: var(--bubble); }
.body { flex: 1; overflow-y: auto; padding: 14px; display: flex; flex-direction: column; gap: 10px; }
.intro, .note, .context, .status, .counter { color: var(--muted); font-size: 13px; }
.intro { color: var(--fg); }
.context { border: 1px solid var(--line); border-radius: 8px; padding: 8px 10px; }
.context label { display: flex; gap: 8px; align-items: flex-start; margin-top: 6px; cursor: pointer; color: var(--fg); }
.context input { margin: 3px 0 0; }
.msg { max-width: 88%; padding: 8px 11px; border-radius: 10px; white-space: pre-wrap; overflow-wrap: anywhere; }
.msg.visitor { align-self: flex-end; background: var(--accent); color: var(--accent-fg); }
.msg.assistant { align-self: flex-start; background: var(--bubble); }
.redacted { align-self: flex-end; font-size: 12px; color: var(--muted); margin-top: -6px; }
.mismatch { font-size: 12px; background: var(--warn-bg); color: var(--warn-fg); border-radius: 8px; padding: 6px 9px; }
.status[role=alert] { color: var(--warn-fg); background: var(--warn-bg); border-radius: 8px; padding: 6px 9px; }
.composer { border-top: 1px solid var(--line); padding: 10px 12px; display: flex; flex-direction: column; gap: 8px; }
.composer textarea {
  width: 100%; min-height: 64px; max-height: 140px; resize: vertical; padding: 8px 10px;
  border: 1px solid var(--line); border-radius: 8px; background: var(--bg);
}
.actions { display: flex; justify-content: space-between; align-items: center; gap: 8px; }
.primary { border: 0; border-radius: 8px; padding: 8px 14px; background: var(--accent); color: var(--accent-fg); font-weight: 600; }
.primary:disabled { opacity: .55; cursor: default; }
.secondary { border: 1px solid var(--line); border-radius: 8px; padding: 7px 12px; background: transparent; }
.done { padding: 24px 14px; display: flex; flex-direction: column; gap: 10px; }
.done p { margin: 0; }
.ref { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 13px; }
.sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }
`;
