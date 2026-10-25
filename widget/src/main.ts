// Entry point for the IIFE. One tag:
//   <script src="https://feedback.example.com/widget/intake.js" data-site-key="…" defer></script>
// Optional: data-api="https://other-host" when the API is not on the script's origin.
import { createApi } from "./api";
import { collectContext } from "./context";
import { createConversation } from "./conversation";
import { makeScrubberLoader } from "./scrub/loader";
import { mountWidget } from "./ui/widget";

// `document.currentScript` is only set while this script executes; capture it now.
const script = document.currentScript as HTMLScriptElement | null;

function boot(): void {
  const siteKey = script?.dataset["siteKey"];
  if (!script || !siteKey) {
    console.error("[intake] missing data-site-key on the script tag; widget not mounted");
    return;
  }
  const scriptUrl = script.src;
  const apiBase = script.dataset["api"] ?? new URL(scriptUrl).origin;

  const widget = mountWidget({
    host: document.body,
    describeContext: () => collectContext(),
    onSend: (text) => conversation.send(text),
    onEnd: () => conversation.end(),
  });

  const conversation = createConversation({
    api: createApi(apiBase),
    siteKey,
    loadScrubber: makeScrubberLoader(scriptUrl),
    context: () => (widget.includeContext() ? collectContext() : null),
    onChange: (snapshot) => widget.render(snapshot),
  });
}

if (document.body) {
  boot();
} else {
  document.addEventListener("DOMContentLoaded", boot, { once: true });
}
