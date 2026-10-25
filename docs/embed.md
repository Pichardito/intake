# Embedding the widget

One script tag. The server serves `intake.js` at `/widget/intake.js` on the intake host (for
the first site, `https://feedback.morningqueue.com`); the Veilkit chunk (`veilkit-loader.js`,
`veilkit.js`, `veilkit_bg.wasm`) sits next to it and is fetched only when a visitor opens the
panel. Each site has its own public `data-site-key`; the server enforces that site's origin
allowlist, so a leaked key is useless from another domain.

```html
<script src="https://feedback.morningqueue.com/widget/intake.js" data-site-key="SITE_KEY" defer></script>
```

Optional `data-api="https://other-host"` points the widget at an API on a different origin than
the script (not needed when both are served by intake-server).

The widget renders inside a shadow root, adds one fixed-position element (`#intake-widget`) to
`<body>`, uses no external fonts, and follows `prefers-color-scheme` and
`prefers-reduced-motion`. It collects nothing automatically except page URL, viewport and user
agent, which the visitor sees and can switch off before the first message.

## Astro (Morning Queue marketing site, `apps/site`)

Put the tag in the shared layout so every page gets it. Astro renders it as plain HTML; `is:inline`
stops Astro from bundling or hoisting it.

```astro
---
// src/layouts/Base.astro
const INTAKE_KEY = import.meta.env.PUBLIC_INTAKE_SITE_KEY;
---
<html lang="en">
  <head>…</head>
  <body>
    <slot />
    {INTAKE_KEY && (
      <script is:inline src="https://feedback.morningqueue.com/widget/intake.js" data-site-key={INTAKE_KEY} defer></script>
    )}
  </body>
</html>
```

## React SPA (Morning Queue app, `apps/web`)

Load it once, after sign-in, from the root layout. The widget reads `document.currentScript`, so it
must be a real script element, not an `import()`.

```tsx
// src/components/IntakeWidget.tsx
import { useEffect } from "react";

const SRC = "https://feedback.morningqueue.com/widget/intake.js";

export function IntakeWidget({ siteKey }: { siteKey: string }) {
  useEffect(() => {
    if (document.querySelector(`script[src="${SRC}"]`)) return;
    const s = document.createElement("script");
    s.src = SRC;
    s.defer = true;
    s.dataset.siteKey = siteKey;
    document.body.appendChild(s);
    // Intentionally not removed on unmount: the widget is page-global and keeps its conversation.
  }, [siteKey]);
  return null;
}
```

```tsx
// in the authenticated root layout
{user && <IntakeWidget siteKey={import.meta.env.VITE_INTAKE_SITE_KEY} />}
```

## Next.js (Code Warrior)

Use `next/script` in the root layout with `afterInteractive` so it never blocks hydration.

```tsx
// app/layout.tsx
import Script from "next/script";

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>
        {children}
        <Script
          src="https://feedback.codewarrior.example/widget/intake.js"
          data-site-key={process.env.NEXT_PUBLIC_INTAKE_SITE_KEY}
          strategy="afterInteractive"
        />
      </body>
    </html>
  );
}
```

(Replace the host with the subdomain CNAMEd to the intake app for that site; the server is
multi-site, one deployment serves all three.)

## Content Security Policy

If the host page sets a CSP, allow the intake origin in these directives (replace the host):

| Directive | Value | Why |
|---|---|---|
| `script-src` | `https://feedback.morningqueue.com` | `intake.js` and the lazily imported `veilkit-loader.js` / `veilkit.js` |
| `connect-src` | `https://feedback.morningqueue.com` | the `/v1/conversations` API calls and the `veilkit_bg.wasm` fetch |
| `script-src` | `'wasm-unsafe-eval'` | required by browsers to compile WebAssembly under a CSP; it does not enable JavaScript `eval` |

Everything else stays inside the shadow root: styles are injected as a `<style>` element in the
shadow tree (no `style-src` change needed unless the page uses a strict `style-src` without
`'unsafe-inline'`, in which case add a nonce via the host's usual mechanism or allow the intake
origin). No images, fonts, frames or workers are loaded.

Example:

```
Content-Security-Policy:
  default-src 'self';
  script-src 'self' https://feedback.morningqueue.com 'wasm-unsafe-eval';
  connect-src 'self' https://feedback.morningqueue.com;
```

## Local development

`pnpm -C widget dev` starts the API mock on `http://127.0.0.1:8787` with a demo host page at
`/demo` (run `pnpm -C widget build` first; the mock serves `widget/dist`). The console mock login
is `http://127.0.0.1:8787/admin/login`.
