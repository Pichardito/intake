// Resolve playwright-core without adding it as a dependency: it ships with the
// globally installed Playwright MCP, and its Chromium is already downloaded.
// Override with PLAYWRIGHT_CORE=/path/to/playwright-core when that moves.
import { createRequire } from "node:module";

const DEFAULT = "/opt/homebrew/lib/node_modules/@playwright/mcp/node_modules/playwright-core";
const require = createRequire(import.meta.url);

export function loadPlaywright() {
  const path = process.env.PLAYWRIGHT_CORE ?? DEFAULT;
  return require(path);
}
