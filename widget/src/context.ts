import type { PageContext } from "./conversation";

/** What the widget attaches to a conversation: page URL, viewport, browser. Nothing else. */
export function collectContext(win: Window = window): PageContext {
  return {
    page_url: win.location.href,
    ua: win.navigator.userAgent,
    viewport: `${win.innerWidth}x${win.innerHeight}`,
  };
}

/** A short browser name for the context row; the full UA string still goes to the server. */
export function browserName(ua: string): string {
  if (/Edg\//.test(ua)) return "Edge";
  if (/OPR\//.test(ua)) return "Opera";
  if (/Firefox\//.test(ua)) return "Firefox";
  if (/Chrome\//.test(ua)) return "Chrome";
  if (/Safari\//.test(ua)) return "Safari";
  return "your browser";
}
