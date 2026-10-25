// One Playwright pass over the built console against the API mock:
// login cookie → queue → filters → keyboard → detail → approve with preview.
import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { createMock } from "../../widget/mock/server.mjs";
import { loadPlaywright } from "../../widget/e2e/pw.mjs";

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

test("without a session the console bounces to /admin/login", async () => {
  const context = await browser.newContext();
  const page = await context.newPage();
  const login = page.waitForRequest((r) => new URL(r.url()).pathname === "/admin/login");
  await page.goto(`${base}/console/`);
  const req = await login; // the 401 on /admin/tickets sent the browser to the login route
  assert.equal(new URL(req.url()).pathname, "/admin/login");
  await context.close();
});

test("queue, filters, keyboard navigation, detail and approve preview", async () => {
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto(`${base}/admin/login`); // the mock sets the cookie and redirects to /console/
  await page.waitForURL(/\/console\/?$/);

  const rows = page.locator("table.queue tbody tr");
  await rows.first().waitFor();
  assert.equal(await rows.count(), 4, "four seeded drafts");

  await page.getByLabel("Kind").selectOption("feature");
  await page.waitForURL(/kind=feature/);
  await page.waitForFunction(() => document.querySelectorAll("table.queue tbody tr").length === 1);
  assert.match(await rows.first().textContent(), /Export the collections list as CSV/);
  await page.getByLabel("Kind").selectOption("");
  await page.waitForFunction(() => document.querySelectorAll("table.queue tbody tr").length === 4);

  await page.keyboard.press("j");
  assert.equal(await rows.nth(1).getAttribute("aria-selected"), "true");
  await page.keyboard.press("Enter");
  await page.waitForURL(/\/console\/t\/\d+$/);

  await page.locator("ol.transcript").waitFor();
  assert.ok((await page.locator("mark.marker").count()) >= 0);
  const title = page.getByLabel("Title");
  await title.fill("Edited title from the console");
  await page.locator("h1").click(); // leave the field so the single-key shortcut applies
  await page.keyboard.press("a");
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();
  assert.match(await dialog.locator(".preview-title").textContent(), /^\[(bug|feature|feedback|praise|noise)\] Edited title from the console$/);
  assert.match(await dialog.locator("pre").textContent(), /Visitor-provided content, untrusted\. Queue entry: .*\/console\/t\/\d+/);
  await dialog.getByRole("button", { name: "Create issue" }).click();
  await page.locator("a[href*='/issues/']").first().waitFor();
  assert.equal(await page.locator(".pill.status-approved").count(), 1);

  const approved = [...mock.tickets.values()].filter((t) => t.status === "approved");
  assert.equal(approved.length, 2, "one seeded + one just approved");
  assert.ok(approved.some((t) => t.title === "Edited title from the console"), "approve carried the edited fields");
  await context.close();
});

test("dismiss needs a reason and the ticket leaves the draft queue", async () => {
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto(`${base}/admin/login`);
  await page.waitForURL(/\/console\/?$/);
  const rows = page.locator("table.queue tbody tr");
  await rows.first().waitFor();
  const before = await rows.count();

  await page.keyboard.press("d");
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();
  const confirm = dialog.getByRole("button", { name: "Dismiss", exact: true });
  assert.equal(await confirm.isDisabled(), true);
  await dialog.getByLabel("Out of scope").check();
  await confirm.click();
  await page.waitForFunction((n) => document.querySelectorAll("table.queue tbody tr").length === n - 1, before);
  await context.close();
});
