// End to end against the mock: a real browser, the built IIFE, real Veilkit wasm.
// Asserts that an SSN and an email typed by the visitor never reach the network.
import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { createMock } from "../mock/server.mjs";
import { loadPlaywright } from "./pw.mjs";

const SSN = "078-05-1120";
const EMAIL = "jane.doe@example.com";

let mock;
let base;
let browser;

before(async () => {
  mock = createMock();
  base = await mock.listen(0);
  const { chromium } = loadPlaywright();
  browser = await chromium.launch({ headless: true });
});

after(async () => {
  await browser?.close();
  await mock?.close();
});

async function openPanel(page) {
  await page.goto(`${base}/demo`);
  const launcher = page.locator("#intake-widget button.launcher");
  await launcher.click();
  const dialog = page.locator("#intake-widget [role=dialog]");
  await dialog.waitFor();
  return { dialog, launcher };
}

test("an SSN and an email are redacted in the request body and shown as redacted", async () => {
  const page = await browser.newPage();
  const bodies = [];
  page.on("request", (req) => {
    if (req.url().includes("/v1/")) bodies.push({ url: req.url(), body: req.postData() ?? "" });
  });

  const { dialog } = await openPanel(page);
  const textarea = dialog.locator("textarea");
  await textarea.fill(`My SSN is ${SSN} and my email is ${EMAIL}. The export button does nothing.`);
  await textarea.press("Enter");

  const line = dialog.locator(".redacted");
  await line.waitFor();
  assert.equal(await line.textContent(), "redacted: EMAIL ×1, SSN ×1");

  await dialog.locator(".msg.assistant").first().waitFor();
  const msgReq = bodies.find((b) => b.url.endsWith("/messages"));
  assert.ok(msgReq, "a message request was sent");
  const parsed = JSON.parse(msgReq.body);
  assert.equal(parsed.text, "My SSN is [SSN] and my email is [EMAIL]. The export button does nothing.");
  assert.equal(parsed.source, "text");

  const everything = JSON.stringify(bodies);
  assert.ok(!everything.includes(SSN), "raw SSN never left the browser");
  assert.ok(!everything.includes(EMAIL), "raw email never left the browser");
  assert.equal(await dialog.locator(".mismatch").count(), 0);
  assert.match(await dialog.locator(".counter").textContent(), /Turn 1 of 12/);
  await page.close();
});

test("the wasm chunk loads lazily, only when the panel opens", async () => {
  const page = await browser.newPage();
  const loaded = [];
  page.on("response", (res) => loaded.push(new URL(res.url()).pathname));
  await page.goto(`${base}/demo`);
  await page.locator("#intake-widget button.launcher").waitFor();
  assert.ok(loaded.includes("/widget/intake.js"));
  assert.ok(!loaded.some((p) => p.endsWith(".wasm")), "wasm not fetched before open");

  await page.locator("#intake-widget button.launcher").click();
  const dialog = page.locator("#intake-widget [role=dialog]");
  await dialog.locator("textarea").fill("hello");
  await dialog.locator("textarea").press("Enter");
  await dialog.locator(".msg.assistant").first().waitFor();
  assert.ok(loaded.some((p) => p.endsWith("/widget/veilkit_bg.wasm")), "wasm fetched after first send");
  await page.close();
});

test("the context row is sent unless the visitor opts out", async () => {
  const page = await browser.newPage();
  const starts = [];
  page.on("request", (req) => {
    if (req.url().endsWith("/v1/conversations")) starts.push(JSON.parse(req.postData() ?? "{}"));
  });
  const { dialog } = await openPanel(page);
  await dialog.locator("input.omit-context").check();
  await dialog.locator("textarea").fill("no context please");
  await dialog.locator("button.primary", { hasText: "Send" }).click();
  await dialog.locator(".msg.assistant").first().waitFor();
  assert.deepEqual(starts[0], { site_key: "sk_site_demo" });
  await page.close();
});

test("a server that redacts more than the browser triggers the mismatch warning", async () => {
  const page = await browser.newPage();
  const { dialog } = await openPanel(page);
  await dialog.locator("textarea").fill("MISMATCH this should warn");
  await dialog.locator("textarea").press("Enter");
  await dialog.locator(".mismatch").waitFor();
  await page.close();
});

test("the conversation ends with a reference id after the persona closes", async () => {
  const page = await browser.newPage();
  const { dialog } = await openPanel(page);
  for (const text of ["bug one", "doing x", "expected y"]) {
    await dialog.locator("textarea").fill(text);
    await dialog.locator("textarea").press("Enter");
    await dialog.locator(".msg.visitor", { hasText: text }).waitFor();
  }
  // The fourth visitor turn answers the third follow-up; the persona closes and the done view replaces the transcript.
  await dialog.locator("textarea").fill("got z");
  await dialog.locator("textarea").press("Enter");
  await dialog.locator(".done").waitFor();
  const done = await dialog.locator(".done").textContent();
  assert.match(done, /Thanks, we've got it\. We read every one\./);
  assert.match(done, /Reference: \d+/);
  assert.equal(await dialog.locator("textarea").count(), 0);
  assert.equal(mock.tickets.size, 7, "the mock created a draft ticket");
  await page.close();
});

test("keyboard: dialog semantics, Escape closes, focus returns to the launcher", async () => {
  const page = await browser.newPage();
  const { dialog, launcher } = await openPanel(page);
  assert.equal(await dialog.getAttribute("aria-modal"), "true");
  assert.equal(await launcher.getAttribute("aria-expanded"), "true");
  const focusedIsTextarea = await page.evaluate(() => {
    const root = document.getElementById("intake-widget")?.shadowRoot;
    return root?.activeElement?.tagName === "TEXTAREA";
  });
  assert.equal(focusedIsTextarea, true);
  await page.keyboard.press("Escape");
  assert.equal(await dialog.count(), 0);
  const focusedIsLauncher = await page.evaluate(() => {
    const root = document.getElementById("intake-widget")?.shadowRoot;
    return root?.activeElement?.classList.contains("launcher");
  });
  assert.equal(focusedIsLauncher, true);
  await page.close();
});

test("no 'AI' wording anywhere in the widget text", async () => {
  const page = await browser.newPage();
  const { dialog } = await openPanel(page);
  const text = await dialog.evaluate((el) => el.textContent ?? "");
  assert.doesNotMatch(text, /\bAI\b/);
  assert.doesNotMatch(text, /powered by/i);
  await page.close();
});
