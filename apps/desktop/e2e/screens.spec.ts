import { readFileSync, writeFileSync } from "node:fs";
import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";

// Baselines are pixel-exact for the browser build that made them. Text
// rasterises differently across Chromium builds, so on another build the
// walk still runs every check and saves the screenshots, but skips the pixel
// comparison and says so. `just e2e-update` regenerates them and this file.
const BROWSER_FILE = new URL("./baseline/BROWSER", import.meta.url);
const baselineBrowser = (() => {
  try {
    return readFileSync(BROWSER_FILE, "utf8").trim();
  } catch {
    return "";
  }
})();

// WP-10 acceptance: every v1 screen is reachable in direction A's chrome on
// the recorded core, passes axe in both appearances, matches its committed
// screenshot baseline, and its main list works from the keyboard.

const SCREENS = [
  { route: "overview", title: "Overview" },
  { route: "inbox", title: "Inbox" },
  { route: "invoices", title: "Invoices" },
  { route: "purchases", title: "Purchases" },
  { route: "bank", title: "221 · ČSOB Business ··4412" },
  { route: "statements", title: "Profit and loss" },
  { route: "taxes", title: "Taxes" },
  { route: "advisors", title: "Advisors" },
  { route: "register", title: "Egress register" },
  { route: "settings", title: "Settings" },
] as const;

// Every test runs clean: an error the page throws or logs (React's recovered
// render errors included) fails it, even when the screen looks right.
const pageErrors = new WeakMap<Page, string[]>();
test.beforeEach(({ page }) => {
  const errors: string[] = [];
  pageErrors.set(page, errors);
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
});
test.afterEach(({ page }) => {
  expect(pageErrors.get(page) ?? [], "errors in the page").toEqual([]);
});

async function axeClean(page: Page) {
  const results = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
    .analyze();
  expect(
    results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target).join(", ")}`),
  ).toEqual([]);
}

/** Compares against the committed baseline when this Chromium made it. */
async function baseline(page: Page, name: string) {
  const browser = page.context().browser()?.version() ?? "unknown";
  if (test.info().config.updateSnapshots === "all") writeFileSync(BROWSER_FILE, `${browser}\n`);
  if (browser === baselineBrowser || test.info().config.updateSnapshots === "all") {
    await expect(page).toHaveScreenshot(`${name}.png`, { maxDiffPixels: 50 });
  } else {
    test.info().annotations.push({
      type: "visual baseline",
      description: `made with Chromium ${baselineBrowser || "?"}; this run uses ${browser}, so pixels weren't compared`,
    });
    await page.screenshot({ path: `test-results/screens/${name}.png` });
  }
}

async function settle(page: Page) {
  await expect(page.getByText("Loading…")).toHaveCount(0);
  await expect(page.getByTestId("status-line")).toContainText("chain verified");
  await page.evaluate(() => document.fonts.ready);
}

for (const scheme of ["light", "dark"] as const) {
  test.describe(`${scheme} appearance`, () => {
    test.use({ colorScheme: scheme });

    for (const { route, title } of SCREENS) {
      test(`${route} renders in the three-pane chrome and passes axe`, async ({ page }) => {
        await page.goto(`/#/${route}`);
        await expect(page.getByRole("heading", { level: 1, name: title })).toBeVisible();
        await expect(page.getByRole("navigation", { name: "Sections" })).toBeVisible();
        await expect(page.getByRole("complementary")).toBeVisible();
        await expect(page.getByRole("option", { selected: true })).toHaveAttribute(
          "data-key",
          route,
        );
        await settle(page);

        await axeClean(page);
        await baseline(page, `${route}-${scheme}`);
      });
    }

    test("the invoice editor passes axe with the core's problems showing", async ({ page }) => {
      await page.goto("/#/invoices/new");
      await settle(page);
      const form = page.getByRole("form", { name: "New invoice" });
      await form.getByLabel("Line 1 description").fill("UX audit");
      await form.getByLabel("Line 1 quantity").fill("12");
      await form.getByLabel("Line 1 unit price").fill("1450.00");
      await page.getByRole("button", { name: "Save draft" }).click();
      await expect(form.getByRole("alert")).toContainText("pick a customer");
      await axeClean(page);
      await baseline(page, `invoice-editor-${scheme}`);
    });
  });
}

test("the source list navigates by keyboard and keeps focus", async ({ page }) => {
  await page.goto("/#/overview");
  await settle(page);
  const sections = page.getByRole("listbox", { name: "Sections" });
  await sections.getByRole("option", { name: "Overview" }).focus();
  await page.keyboard.press("ArrowDown");
  await expect(page).toHaveURL(/#\/inbox$/);
  await expect(sections.getByRole("option", { name: /Inbox/ })).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(page).toHaveURL(/#\/invoices$/);
  await expect(page.getByRole("heading", { level: 1, name: "Invoices" })).toBeVisible();
  await expect(sections.getByRole("option", { name: /Invoices/ })).toBeFocused();
});

test("main lists move the selection and the inspector by keyboard", async ({ page }) => {
  await page.goto("/#/inbox");
  await settle(page);
  const inbox = page.getByRole("grid", { name: "Inbox" });
  // The first proposal is selected by default; clicking it focuses the list.
  const first = inbox.getByRole("row", { name: /Google Ireland/ });
  await first.click();
  await expect(first).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("complementary", { name: /Google Ireland/ })).toContainText(
    "Proposed journal entry",
  );
  await expect(page.getByRole("complementary")).toContainText("242,11");
  await page.keyboard.press("ArrowDown");
  await expect(page).toHaveURL(/#\/inbox\/p-alza$/);

  await page.goto("/#/bank");
  const lines = page.getByRole("grid", { name: "Bank lines" });
  // Newest first: the account fee (6 Oct), then Alza (5 Oct).
  await lines.getByRole("row", { name: /Poplatek za vedení účtu/ }).click();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("complementary")).toContainText("Alza.cz a.s.");
});

test("deep links select the item and statements drill into their entries", async ({ page }) => {
  await page.goto("/#/invoices/2026-102");
  const inspector = page.getByRole("complementary", { name: "Invoice 2026-102" });
  await expect(inspector).toContainText("Overdue 13 days");
  await expect(inspector).toContainText("53 092,00");

  await page.goto("/#/statements");
  const pnl = page.getByRole("table", { name: "Profit and loss" });
  await expect(pnl).toContainText("456 500,00");
  await expect(pnl).toContainText("280 350,00");
  await expect(pnl).toContainText("278 780,00");
  await pnl.getByRole("button", { name: "Sales of services" }).click();
  await expect(page).toHaveURL(/#\/statements\/602$/);
  await expect(page.getByRole("complementary", { name: "Account 602" })).toContainText(
    "Entries behind this line · 4",
  );
});

test("figures on screen are the core's: overview, bank tie-out, cash basis", async ({ page }) => {
  await page.goto("/#/overview");
  await expect(page.getByRole("grid", { name: "Key figures" })).toContainText("280 350,00 Kč");
  await page.goto("/#/bank");
  await expect(page.getByTestId("tie-out")).toContainText("Ties to the bank's closing");
  await expect(page.getByTestId("tie-out")).toContainText("849 649,58");
  await page.goto("/#/taxes/taxable-income");
  await expect(page.getByRole("complementary", { name: "Taxable income" })).toBeVisible();
});

test("the September DPH return comes from the ledger, mapped by the rule pack", async ({
  page,
}) => {
  await page.goto("/#/taxes/vat-2026-09");
  const inspector = page.getByRole("complementary", { name: "DPH return · September 2026" });
  await expect(inspector).toContainText("9 642,57 Kč");
  await expect(inspector).toContainText("26 Oct 2026");
  const rows = inspector.getByRole("table", { name: "DPH return rows" });
  await expect(rows).toContainText("ř. 43");
  await expect(rows).toContainText("12 318,60");
  await expect(inspector).toContainText("cz-2026@2026.1 (draft)");

  await page.goto("/#/settings/rule-pack");
  const pack = page.getByRole("complementary", { name: "Rule pack" });
  await expect(pack).toContainText("vat.rate.reduced");
  await expect(pack).toContainText(
    "Zákon č. 235/2004 Sb., o dani z přidané hodnoty, § 47 odst. 1 písm. b)",
  );
  await expect(pack).toContainText("Left out on purpose");
});

test("the September kontrolní hlášení itemises above the pack's threshold and agrees with the return", async ({
  page,
}) => {
  await page.goto("/#/taxes/kh-2026-09");
  const inspector = page.getByRole("complementary", {
    name: "Kontrolní hlášení · September 2026",
  });
  await expect(inspector).toContainText("10 000,00 Kč including VAT are itemised");
  await expect(inspector.getByRole("table", { name: "A.4 supplies" })).toContainText("2026-114");
  await expect(inspector.getByRole("table", { name: "A.2 services from the EU" })).toContainText(
    "AWS-2026-09",
  );
  await expect(inspector.getByRole("table", { name: "B.2 purchases" })).toContainText(
    "PF-2026-0917",
  );
  await expect(inspector).toContainText("Section C agrees with the DPH return");
  await expect(inspector).toContainText("26 Oct 2026");
  await axeClean(page);
});

test("the scenario engine compares actual and flat-rate expenses, side effects included", async ({
  page,
}) => {
  await page.goto("/#/taxes/scenarios");
  const inspector = page.getByRole("complementary", { name: "Actual or flat-rate expenses" });
  await expect(inspector.getByTestId("scenario-source")).toContainText("From the books, 1 Jan");
  const form = inspector.getByRole("form", { name: "Projection" });
  await form.getByLabel("Income (§ 7)").fill("1 571 000");
  await form.getByLabel("Actual expenses").fill("383 200");
  await form.getByLabel("Planned purchase").fill("Laptop");
  await form.getByLabel("Its price excluding VAT").fill("60 000");
  await form.getByRole("button", { name: "Compare" }).click();
  await expect(inspector.getByTestId("scenario-source")).toContainText("Your projection for 2026");
  const flat = inspector
    .locator("section", {
      has: page.getByRole("heading", { name: "Flat-rate expenses 60 % · Laptop bought this year" }),
    })
    .last();
  await expect(flat).toContainText("942 600,00 Kč");
  await expect(flat).toContainText("197 584,00 Kč");
  await expect(flat).toContainText("Lowest total");
  await expect(inspector).toContainText("Paušální daň");
  await axeClean(page);
  await form.getByLabel("Income (§ 7)").fill("1.571.000");
  await form.getByRole("button", { name: "Compare" }).click();
  await expect(form.getByRole("alert")).toContainText("isn't an amount");
});

test("the obligations calendar comes from the pack and moves deadlines past holidays", async ({
  page,
}) => {
  await page.goto("/#/taxes/calendar");
  const inspector = page.getByRole("complementary", { name: "Obligations calendar 2026" });
  const may = inspector.getByRole("list", { name: "Deadlines in May" });
  await expect(may).toContainText("Income tax return (DPFO), filed electronically");
  await expect(may).toContainText("moved from 1 May");
  const october = inspector.getByRole("list", { name: "Deadlines in October" });
  await expect(october.getByRole("listitem").filter({ hasText: "Next" })).toContainText(
    "Health insurance advance",
  );
  await expect(october).toContainText("moved from 25 Oct");
  await axeClean(page);
});

test("the advisors say what they run on, from the core's provider status", async ({ page }) => {
  await page.goto("/#/advisors/tax");
  const connection = page.getByRole("region", { name: "Advisor connection" });
  await expect(connection).toContainText("Demo · recorded runs");
  await expect(connection).toContainText("no model is called");
  await expect(page.getByRole("grid", { name: "Advisors" })).toContainText("Demo · recorded runs");
});

test("the register shows exactly what was shared, with identifiers withheld", async ({ page }) => {
  await page.goto("/#/register/run-2026-10-04-02");
  const inspector = page.getByRole("complementary", {
    name: "Classify a bank line without a reference",
  });
  const payload = inspector.getByTestId("payload");
  await expect(payload).toContainText("list_unmatched_bank_lines");
  await expect(payload).toContainText("Vendor");
  await expect(payload).not.toContainText(/CZ\d{2}/);
  await expect(inspector).toContainText("counterpartyAccount");
  await expect(inspector).toContainText("Unaltered (hash chain verified)");
  await axeClean(page);
});

test("a task can be stopped from sharing anything", async ({ page }) => {
  await page.goto("/#/settings/advisor-sharing");
  const inspector = page.getByRole("complementary", { name: "What advisors may send" });
  await inspector
    .getByRole("button", { name: /Tax scenarios and explanations: when it runs/ })
    .click();
  await page.getByRole("option", { name: "Never run" }).click();
  await expect(page.getByRole("grid", { name: "Settings" })).toContainText("3 of 4 tasks allowed");
  await axeClean(page);
});

test("the tax advisor explains the scenarios, and every figure was checked", async ({ page }) => {
  await page.goto("/#/taxes/scenarios");
  const inspector = page.getByRole("complementary", { name: "Actual or flat-rate expenses" });
  await inspector.getByRole("button", { name: "Explain with the tax advisor…" }).click();
  await expect(page.getByRole("dialog")).toContainText("are never sent");
  await page.getByRole("button", { name: "Send and explain" }).click();
  const advice = inspector.getByTestId("tax-advice");
  await expect(advice).toContainText("Draft for your review");
  await expect(advice).toContainText("80 324 Kč less");
  await expect(advice).toContainText("figures checked against the engine");
  await axeClean(page);

  // The review's projection, then what was shared.
  const form = inspector.getByRole("form", { name: "Projection" });
  await form.getByLabel("Income (§ 7)").fill("1 571 000");
  await form.getByLabel("Actual expenses").fill("383 200");
  await form.getByLabel("Planned purchase").fill("Laptop");
  await form.getByLabel("Its price excluding VAT").fill("60 000");
  await form.getByRole("button", { name: "Compare" }).click();
  await inspector.getByRole("button", { name: "Explain with the tax advisor…" }).click();
  await page.getByRole("button", { name: "Send and explain" }).click();
  await expect(inspector.getByTestId("tax-advice")).toContainText("197 584 Kč");
  await inspector.getByRole("button", { name: "What was shared" }).click();
  await expect(
    page.getByRole("complementary", { name: "Explain the tax scenarios" }),
  ).toContainText("run_scenario");
});

test("the financial advisor's detectors find the 42 % rise in subcontracting", async ({ page }) => {
  await page.goto("/#/advisors/financial");
  const list = page.getByRole("list", { name: "Findings" });
  await expect(list).toContainText("Subcontracting rose 42 % on the previous quarter");
  await expect(list).toContainText("74 700,00 Kč → 106 100,00 Kč");
  await expect(list).toContainText("Entries #8, #9, #10, #34, #35, #36");
  await expect(list).toContainText("Figma charged twice, and refunded");
  await axeClean(page);
});

test("explain this cites the entries behind a figure", async ({ page }) => {
  await page.goto("/#/statements/518");
  const inspector = page.getByRole("complementary", { name: "Account 518" });
  await inspector.getByRole("button", { name: "Explain this…" }).click();
  await page.getByRole("button", { name: "Send and explain" }).click();
  const explanation = inspector.getByTestId("explanation");
  await expect(explanation).toContainText("120 800 Kč");
  await expect(explanation).toContainText("Cites #34, #35, #36, #39, #40, #41, #42, #43");
  await expect(inspector.locator("li[data-cited]")).toHaveCount(8);
  await axeClean(page);
});

test("first run: set up encrypted books, confirm the recovery key, unlock, and recover", async ({
  page,
}) => {
  await page.goto("/#/setup");
  const who = page.getByRole("form", { name: "Who the books are for" });
  await who.getByLabel("Name").fill("Eva Malá");
  await who.getByLabel("IČO").fill("27415830");
  await who.getByLabel("DIČ").fill("CZ8001011234");
  await who.getByLabel("Address").fill("Dlouhá 1, 110 00 Praha 1");
  await who.getByRole("button", { name: /Flat-rate group/ }).click();
  await page.getByRole("option", { name: "Other self-employment (40 %)" }).click();
  await who.getByLabel("Trade register line").fill("Zapsána v živnostenském rejstříku");
  await who.getByLabel("Business account IBAN").fill("CZ6508000000192000145399");
  await who.getByLabel("Bank").fill("ČSOB");
  await axeClean(page);
  await who.getByRole("button", { name: "Next" }).click();

  const pass = page.getByRole("form", { name: "Passphrase" });
  await pass.getByLabel("Passphrase").fill("short");
  await pass.getByLabel("The same again").fill("short");
  await pass.getByRole("button", { name: "Create the books" }).click();
  await expect(pass.getByRole("alert")).toContainText("at least 10 characters");
  await pass.getByLabel("Passphrase").fill("a long passphrase for the books");
  await pass.getByLabel("The same again").fill("a long passphrase for the books");
  await pass.getByRole("button", { name: "Create the books" }).click();

  const key = (await page.getByTestId("recovery-key").textContent()) ?? "";
  expect(key.split("-")).toHaveLength(13);
  await axeClean(page);
  await page.getByRole("button", { name: "I've saved it" }).click();
  const confirm = page.getByRole("form", { name: "Confirm the recovery key" });
  await confirm.getByLabel(/Type the last group/).fill("AAAA");
  await confirm.getByRole("button", { name: "Confirm" }).click();
  await expect(confirm.getByRole("alert")).toContainText("isn't the last group");
  await confirm.getByLabel(/Type the last group/).fill(key.split("-").at(-1) ?? "");
  await confirm.getByRole("button", { name: "Confirm" }).click();
  await expect(page.getByRole("region", { name: "Your books are ready" })).toContainText(
    "Eva Malá",
  );

  await page.goto("/#/unlock");
  const unlock = page.getByRole("form", { name: "Unlock" });
  await unlock.getByLabel("Passphrase").fill("not the passphrase");
  await unlock.getByRole("button", { name: "Unlock" }).click();
  await expect(unlock.getByRole("alert")).toContainText("doesn't open these books");
  await axeClean(page);
  await unlock.getByLabel("Passphrase").fill("a long passphrase for the books");
  await unlock.getByRole("button", { name: "Unlock" }).click();
  await expect(page).toHaveURL(/#\/overview/);

  await page.goto("/#/recover");
  const recover = page.getByRole("form", { name: "Recover" });
  await recover.getByLabel("Recovery key").fill(key);
  await recover.getByLabel("New passphrase").fill("a brand new passphrase here");
  await recover.getByLabel("The same again").fill("a brand new passphrase here");
  await recover.getByRole("button", { name: "Recover" }).click();
  const fresh = (await page.getByTestId("recovery-key").textContent()) ?? "";
  expect(fresh).not.toBe(key);
  await page.getByRole("button", { name: "I've saved it" }).click();
  await page
    .getByRole("form", { name: "Confirm the recovery key" })
    .getByLabel(/Type the last group/)
    .fill(fresh.split("-").at(-1) ?? "");
  await page.getByRole("button", { name: "Confirm" }).click();
  await expect(page).toHaveURL(/#\/overview/);
});

test("an invoice exports as a PDF the core rendered, with the QR Platba code", async ({ page }) => {
  await page.goto("/#/invoices/2026-102");
  const inspector = page.getByRole("complementary", { name: "Invoice 2026-102" });
  await inspector.getByRole("button", { name: "Export" }).click();
  const downloaded = page.waitForEvent("download");
  await page.getByRole("menuitem", { name: "Czech PDF" }).click();
  const file = await downloaded;
  expect(file.suggestedFilename()).toBe("Faktura 2026-102.pdf");
  const path = await file.path();
  const { readFileSync } = await import("node:fs");
  expect(readFileSync(path).subarray(0, 5).toString()).toBe("%PDF-");
  await expect(inspector.getByRole("status")).toContainText(
    "Saved Faktura 2026-102.pdf with the QR Platba code.",
  );
});

test("an invoice exports as ISDOC for accounting software", async ({ page }) => {
  await page.goto("/#/invoices/2026-102");
  const inspector = page.getByRole("complementary", { name: "Invoice 2026-102" });
  await inspector.getByRole("button", { name: "Export" }).click();
  const downloaded = page.waitForEvent("download");
  await page.getByRole("menuitem", { name: "ISDOC for accounting software" }).click();
  const file = await downloaded;
  expect(file.suggestedFilename()).toBe("2026-102.isdoc");
  const { readFileSync } = await import("node:fs");
  const xml = readFileSync(await file.path(), "utf8");
  expect(xml).toContain('<Invoice xmlns="http://isdoc.cz/namespace/2013" version="6.0.2">');
  expect(xml).toContain("<PayableAmount>103092.00</PayableAmount>");
  await expect(inspector.getByRole("status")).toContainText("Saved 2026-102.isdoc (ISDOC 6.0.2).");
});

for (const [item, file, root, payable] of [
  [
    "UBL for Peppol",
    "2026-102.ubl.xml",
    "<cbc:CustomizationID>urn:cen.eu:en16931:2017#compliant#urn:fdc:peppol.eu:2017:poacc:billing:3.0</cbc:CustomizationID>",
    '<cbc:PayableAmount currencyID="CZK">103092.00</cbc:PayableAmount>',
  ],
  [
    "CII for Factur-X and ZUGFeRD",
    "2026-102.cii.xml",
    "<ram:ID>urn:cen.eu:en16931:2017</ram:ID>",
    "<ram:DuePayableAmount>103092.00</ram:DuePayableAmount>",
  ],
] as const) {
  test(`an invoice exports as ${file}`, async ({ page }) => {
    await page.goto("/#/invoices/2026-102");
    const inspector = page.getByRole("complementary", { name: "Invoice 2026-102" });
    await inspector.getByRole("button", { name: "Export" }).click();
    const downloaded = page.waitForEvent("download");
    await page.getByRole("menuitem", { name: item }).click();
    const download = await downloaded;
    expect(download.suggestedFilename()).toBe(file);
    const { readFileSync } = await import("node:fs");
    const xml = readFileSync(await download.path(), "utf8");
    expect(xml).toContain(root);
    expect(xml).toContain(payable);
  });
}

test("drafts offer no export until issued", async ({ page }) => {
  await page.goto("/#/invoices");
  await page
    .getByRole("row")
    .filter({ hasText: "Acme Analytics" })
    .filter({ hasText: "Draft" })
    .click();
  const inspector = page.getByRole("complementary", { name: "Draft invoice" });
  await expect(inspector.getByRole("button", { name: "Issue" })).toBeVisible();
  await expect(inspector.getByRole("button", { name: "Export" })).toHaveCount(0);
});

// WP-16 acceptance: create → issue → export, on the recorded core. The mock
// replays the core's answers to exactly these steps (skyla_app::recordings).
test("an invoice is created, issued and exported", async ({ page }) => {
  await page.goto("/#/invoices");
  await page.getByRole("button", { name: "New invoice" }).click();
  await expect(page).toHaveURL(/#\/invoices\/new$/);
  const form = page.getByRole("form", { name: "New invoice" });

  // The core checks what was typed and lists every problem.
  await form.getByLabel("Line 1 description").fill("UX audit");
  await form.getByLabel("Line 1 quantity").fill("12");
  await form.getByLabel("Line 1 unit price").fill("1450.00");
  await page.getByRole("button", { name: "Save draft" }).click();
  const alert = form.getByRole("alert");
  await expect(alert).toContainText("pick a customer");
  await expect(alert).toContainText('line 1: unit price "1450.00" isn\'t an amount like 1 200,00');

  await form.getByRole("button", { name: /Customer/ }).click();
  await page.getByRole("option", { name: /Northwind Traders s\.r\.o\./ }).click();
  await form.getByLabel("Line 1 unit price").fill("1 450,00");
  await form.getByRole("button", { name: "Add line" }).click();
  await form.getByLabel("Line 2 description").fill("Workshop");
  await form.getByLabel("Line 2 unit", { exact: true }).fill("ks");
  await form.getByLabel("Line 2 unit price").fill("8 000,00");
  await form.getByLabel("Note on the invoice").fill("Děkuji za spolupráci.");
  await page.getByRole("button", { name: "Save draft" }).click();

  // The draft, with the totals the core computed.
  await expect(page).toHaveURL(/#\/invoices\/draft-\d+$/);
  const draft = page.getByRole("complementary", { name: "Draft invoice" });
  await expect(draft).toContainText("Northwind Traders s.r.o.");
  await expect(draft).toContainText("25 400,00");
  await expect(draft).toContainText("5 334,00");
  await expect(draft).toContainText("30 734,00");

  await draft.getByRole("button", { name: "Issue", exact: true }).click();
  const confirm = page.getByRole("dialog", { name: "Issue invoice" });
  await expect(confirm).toContainText("Issue as 2026-115 on 7 Oct 2026?");
  await confirm.getByRole("button", { name: "Issue invoice" }).click();

  await expect(page).toHaveURL(/#\/invoices\/2026-115$/);
  const issued = page.getByRole("complementary", { name: "Invoice 2026-115" });
  await expect(issued).toContainText("Due 21 Oct");
  await expect(page.getByRole("row", { name: /2026-115/ })).toBeVisible();

  const { readFileSync } = await import("node:fs");
  await issued.getByRole("button", { name: "Export" }).click();
  let downloaded = page.waitForEvent("download");
  await page.getByRole("menuitem", { name: "Czech PDF" }).click();
  let file = await downloaded;
  expect(file.suggestedFilename()).toBe("Faktura 2026-115.pdf");
  expect(
    readFileSync(await file.path())
      .subarray(0, 5)
      .toString(),
  ).toBe("%PDF-");

  await issued.getByRole("button", { name: "Export" }).click();
  downloaded = page.waitForEvent("download");
  await page.getByRole("menuitem", { name: "ISDOC for accounting software" }).click();
  file = await downloaded;
  expect(file.suggestedFilename()).toBe("2026-115.isdoc");
  expect(readFileSync(await file.path(), "utf8")).toContain(
    "<PayableAmount>30734.00</PayableAmount>",
  );
});

test("a draft can be discarded before it's issued", async ({ page }) => {
  await page.goto("/#/invoices/new");
  const form = page.getByRole("form", { name: "New invoice" });
  await form.getByRole("button", { name: /Customer/ }).click();
  await page.getByRole("option", { name: /Acme Analytics a\.s\./ }).click();
  await form.getByLabel("Line 1 description").fill("UX audit");
  await form.getByLabel("Line 1 quantity").fill("12");
  await form.getByLabel("Line 1 unit price").fill("1 450,00");
  await form.getByRole("button", { name: "Add line" }).click();
  await form.getByLabel("Line 2 description").fill("Workshop");
  await form.getByLabel("Line 2 unit", { exact: true }).fill("ks");
  await form.getByLabel("Line 2 unit price").fill("8 000,00");
  await form.getByLabel("Note on the invoice").fill("Děkuji za spolupráci.");
  await page.getByRole("button", { name: "Save draft" }).click();
  await expect(page).toHaveURL(/#\/invoices\/draft-\d+$/);
  await expect(page.getByRole("rowheader", { name: "Draft", exact: true })).toHaveCount(3);

  const draft = page.getByRole("complementary", { name: "Draft invoice" });
  await draft.getByRole("button", { name: "Delete" }).click();
  await page
    .getByRole("dialog", { name: "Delete draft" })
    .getByRole("button", { name: "Delete draft" })
    .click();
  await expect(page).toHaveURL(/#\/invoices$/);
  await expect(page.getByRole("rowheader", { name: "Draft", exact: true })).toHaveCount(2);
});

// WP-19 acceptance: import → accept certain matches → split one → create a
// rule, on the recorded core (skyla_app::recordings, "bank-workbench").
test("a statement is imported, matched, split and turned into a rule", async ({ page }) => {
  await page.goto("/#/bank");
  await settle(page);
  const lines = page.getByRole("grid", { name: "Bank lines" });
  await expect(lines).toContainText("Certain · 1");

  await page
    .getByLabel("Statement file")
    .setInputFiles("../../packages/fixtures/data/statements/csob-2026-10-07.xml");
  await expect(page.getByRole("status")).toContainText(
    "Imported csob-2026-10-07.xml: 3 new lines; it ties out to 885 901,58",
  );
  await expect(lines).toContainText("Certain · 2");
  await lines.getByRole("row", { name: /STUDIO BRNO/ }).click();
  const studio = page.getByRole("complementary", { name: /STUDIO BRNO/ });
  await expect(studio).toContainText("Settle 2026-102");
  await expect(studio).toContainText("VS 2026102 is 2026-102's");
  await expect(studio).toContainText("+45");
  await page.screenshot({ path: "test-results/screens/bank-workbench-imported.png" });

  await page.getByRole("button", { name: "Accept 2 certain" }).click();
  await expect(page.getByRole("status")).toContainText("Accepted 2 certain lines");
  await page.getByRole("radio", { name: "All" }).click();
  await expect(lines).toContainText("Booked · 2");
  await expect(lines).toContainText("Settles 2026-102");

  // Split the Datart purchase across two accounts.
  await lines.getByRole("row", { name: /Datart/ }).click();
  const datart = page.getByRole("complementary", { name: /Datart/ });
  await datart.getByRole("button", { name: "Book…" }).click();
  const split = page.getByRole("dialog", { name: "Book line" });
  await split.getByRole("button", { name: /Row 1 account/ }).click();
  await page.getByRole("option", { name: /^501 / }).click();
  await split.getByLabel("Row 1 amount").fill("3 630,00");
  await split.getByRole("button", { name: "Add row" }).click();
  await split.getByRole("button", { name: /Row 2 account/ }).click();
  await page.getByRole("option", { name: /^518 / }).click();
  await split.getByLabel("Row 2 amount").fill("1 210,00");
  await split.getByRole("button", { name: "Book split" }).click();
  await expect(page.getByRole("status")).toContainText("is booked across 2 accounts");
  await expect(lines).toContainText("Split across 501 + 518");

  // A rule from the rent line.
  await lines.getByRole("row", { name: /Kanceláře Korunní/ }).click();
  const rent = page.getByRole("complementary", { name: /Kanceláře Korunní/ });
  await rent.getByRole("button", { name: "Create rule…" }).click();
  const rule = page.getByRole("dialog", { name: "Create rule" });
  await rule.getByLabel("Rule name").fill("Office rent");
  await rule.getByRole("button", { name: /Book to/ }).click();
  await page.getByRole("option", { name: /^518 / }).click();
  await rule.getByRole("button", { name: "Create rule and book" }).click();
  await expect(page.getByRole("status")).toContainText('Rule "Office rent" created');
  await expect(page.getByTestId("tie-out")).toContainText("1 rule: Office rent");
  await expect(lines).toContainText("Booked · 4");
  await page.screenshot({ path: "test-results/screens/bank-workbench-after.png" });
  await expect(page.getByTestId("status-line")).toContainText("chain verified");
});

// WP-20 acceptance (the UI half): reference data is off by default, comes
// in by hand, and says where it came from.
test("reference data is imported by hand and fetching stays off until turned on", async ({
  page,
}) => {
  await page.goto("/#/settings/reference-data");
  await settle(page);
  const inspector = page.getByRole("complementary", { name: "Public reference data" });
  const toggle = inspector.getByRole("checkbox", { name: /Fetch the ČNB's published rates/ });
  await expect(toggle).not.toBeChecked();
  await expect(inspector).toContainText("none yet: updates can't be installed");

  await inspector
    .getByLabel("ČNB rates file")
    .setInputFiles("../../packages/fixtures/data/refdata/demo-cnb-daily-2026-10-07.txt");
  await expect(inspector.getByRole("status")).toContainText(
    "Imported demo-cnb-daily-2026-10-07.txt",
  );
  await expect(inspector).toContainText("EUR 25,14 Kč on 2026-10-07 (ČNB #194)");
  await inspector
    .getByLabel("Repo-rate history file")
    .setInputFiles("../../packages/fixtures/data/refdata/demo-cnb-repo-history.csv");
  await expect(inspector).toContainText("3,5 % since 2025-05-02");
  await expect(inspector).toContainText("imported demo-cnb-repo-history.csv");

  await inspector.getByText("Fetch the ČNB's published rates").click();
  await expect(toggle).toBeChecked();
  await expect(page.getByRole("row", { name: /Public reference data/ })).toContainText(
    "On · www.cnb.cz",
  );
});

// WP-31 acceptance (import): a Pohoda export is previewed (new, already
// here, won't import, each with its reason) and only the new invoices are
// posted, on the recorded core (skyla_app::recordings, "invoice-import").
test("invoices exported from Pohoda are previewed, then the new ones imported", async ({
  page,
}) => {
  await page.goto("/#/invoices");
  await settle(page);
  await page
    .getByLabel("Invoices exported from Pohoda or Fakturoid")
    .setInputFiles("../../packages/fixtures/data/imports/pohoda-faktury.xml");
  const table = page.getByRole("grid", { name: "Invoices in the file" });
  await expect(page.getByRole("heading", { name: "Import from Pohoda" })).toBeVisible();
  await expect(table).toContainText("Will be imported");
  await expect(table.getByRole("row", { name: /2026-044/ })).toContainText("New");
  await expect(table.getByRole("row", { name: /2026-041/ })).toContainText("Already here");
  await expect(page.getByRole("status")).toContainText("Skipped: item 6 is a received document");
  await table.getByRole("row", { name: /2026-049/ }).click();
  const inspector = page.getByRole("complementary", { name: "Imported invoice" });
  await expect(inspector).toContainText("VAT doesn't match the rate on its tax point");
  await page.screenshot({ path: "test-results/screens/invoice-import-preview.png" });
  await axeClean(page);

  await page.getByRole("button", { name: "Import 2 invoices" }).click();
  await expect(page.getByRole("status")).toContainText(
    "Imported 2 invoices from Pohoda: 2026-044, 2026-047.",
  );
  await expect(page.getByRole("grid", { name: "Invoices" })).toContainText("2026-044");
});

// WP-31 acceptance (export): Settings saves the whole books as one zip.
test("the books are exported as one zip", async ({ page }) => {
  await page.goto("/#/settings/export");
  await settle(page);
  const inspector = page.getByRole("complementary", { name: "Export everything" });
  const downloaded = page.waitForEvent("download");
  await inspector.getByRole("button", { name: "Export…" }).click();
  const file = await downloaded;
  expect(file.suggestedFilename()).toBe("sky-la-export-Jan_Novak-2026-10-07.zip");
  const { readFileSync } = await import("node:fs");
  expect(
    readFileSync(await file.path())
      .subarray(0, 4)
      .toString("hex"),
  ).toBe("504b0304");
  await expect(inspector.getByRole("status")).toContainText(
    "Saved sky-la-export-Jan_Novak-2026-10-07.zip",
  );
});

// WP-33 acceptance (update channel): off until turned on, and with no
// release key in this build it refuses to check, fetching nothing.
test("the update check is opt-in and needs a release key", async ({ page }) => {
  await page.goto("/#/settings/updates");
  await settle(page);
  const inspector = page.getByRole("complementary", { name: "Updates" });
  const toggle = inspector.getByRole("checkbox", { name: /Allow checking github.com/ });
  await expect(toggle).not.toBeChecked();
  await expect(inspector.getByRole("button", { name: "Check now" })).toBeDisabled();
  await expect(inspector).toContainText("This build trusts no release key yet");
  await inspector.getByText("Allow checking github.com").click();
  await expect(toggle).toBeChecked();
  await expect(page.getByRole("row", { name: /Updates/ })).toContainText("On · github.com");
  await inspector.getByRole("button", { name: "Check now" }).click();
  await expect(inspector.getByRole("alert")).toContainText(
    "no release signing key is configured yet",
  );
  await inspector.getByText("Allow checking github.com").click();
  await expect(toggle).not.toBeChecked();
});

// Real books start with no customers: a draft can add one, and the core
// checks it like everything else (skyla_app::recordings, "new-customer").
test("a draft adds a new customer, checked by the core", async ({ page }) => {
  await page.goto("/#/invoices/new");
  await settle(page);
  const form = page.getByRole("form", { name: "New invoice" });
  await form.getByRole("button", { name: /Customer/ }).click();
  await page.getByRole("option", { name: /New customer/ }).click();
  await form.getByLabel("Customer's legal name").fill("Northwind Traders s.r.o.");
  await form.getByLabel("IČO").fill("12345678");
  await form.getByLabel("DIČ").fill("12");
  await form.getByLabel("Line 1 description").fill("UX audit");
  await form.getByLabel("Line 1 quantity").fill("12");
  await form.getByLabel("Line 1 unit price").fill("1 450,00");
  await form.getByRole("button", { name: "Add line" }).click();
  await form.getByLabel("Line 2 description").fill("Workshop");
  await form.getByLabel("Line 2 unit", { exact: true }).fill("ks");
  await form.getByLabel("Line 2 unit price").fill("8 000,00");
  await form.getByLabel("Note on the invoice").fill("Děkuji za spolupráci.");
  await page.getByRole("button", { name: "Save draft" }).click();
  const alert = form.getByRole("alert");
  await expect(alert).toContainText("already a customer");
  await expect(alert).toContainText("IČO 12345678 isn't valid");
  await expect(alert).toContainText("isn't a VAT number");
  await expect(alert).toContainText("address");
  await page.screenshot({ path: "test-results/screens/invoice-new-customer.png" });
  await axeClean(page);

  await form.getByLabel("Customer's legal name").fill("Lesní ateliér s.r.o.");
  await form.getByLabel("IČO").fill("26965313");
  await form.getByLabel("DIČ").fill("CZ26965313");
  await form.getByLabel("Address, as printed on the invoice").fill("Jasmínová 12, 106 00 Praha 10");
  await page.getByRole("button", { name: "Save draft" }).click();
  await expect(page).toHaveURL(/#\/invoices\/draft-\d+$/);
  await expect(page.getByRole("complementary", { name: "Draft invoice" })).toContainText(
    "Lesní ateliér s.r.o.",
  );
});

// Improvement wave 3: a received invoice is recorded, checked by the core
// (skyla_app::recordings, "purchase").
test("a received invoice is recorded, with every problem listed first", async ({ page }) => {
  await page.goto("/#/purchases");
  await settle(page);
  await expect(page.getByRole("grid", { name: "Received invoices" })).toContainText(
    "Kvasnička Dev s.r.o.",
  );
  await page.getByRole("button", { name: "Record received invoice" }).click();
  const form = page.getByRole("form", { name: "Received invoice" });
  await form.getByLabel("Supplier").fill("Kancelářské potřeby Novotný s.r.o.");
  await form.getByLabel("Invoice number").fill("FP-2026-1187");
  await form.getByLabel("IČO").fill("26965313");
  await form.getByLabel("Issued").fill("2026-10-03");
  await form.getByLabel("Due").fill("2026-10-17");
  await form.getByLabel("Line 1 description").fill("Monitor");
  await form.getByLabel("Line 1 amount without VAT").fill("12 000,00");
  await form.getByRole("button", { name: "Add line" }).click();
  await form.getByLabel("Line 2 description").fill("Papír");
  await form.getByLabel("Line 2 amount without VAT").fill("800,00");
  await form.getByLabel("VAT on the invoice, to check (optional)").fill("2 700,00");
  await page.getByRole("button", { name: "Save and post" }).click();
  await expect(form.getByRole("alert")).toContainText("deducting VAT needs the supplier's DIČ");
  await page.screenshot({ path: "test-results/screens/purchase-problems.png" });
  await axeClean(page);

  await form.getByLabel("DIČ").fill("CZ26965313");
  await form.getByLabel("VAT on the invoice, to check (optional)").fill("2 688,00");
  await page.getByRole("button", { name: "Save and post" }).click();
  await expect(page).toHaveURL(/#\/purchases\/\d+$/);
  const inspector = page.getByRole("complementary", { name: "Received invoice FP-2026-1187" });
  await expect(inspector).toContainText("15 488,00");
  await expect(inspector).toContainText("CZ26965313");
});

// Improvement wave 7: business details are editable for real books; the
// demo shows its own, fixed.
test("business details show in Settings, fixed for the demo", async ({ page }) => {
  await page.goto("/#/settings/business");
  await settle(page);
  const inspector = page.getByRole("complementary", { name: "Business details" });
  await expect(inspector).toContainText("The demo's details (fixed)");
  await expect(inspector.getByLabel("Name")).toHaveValue("Jan Novák");
  await expect(inspector.getByLabel("Name")).toBeDisabled();
  await expect(inspector).toContainText("VAT: VAT payer, monthly");
  await expect(inspector.getByRole("button", { name: "Save details" })).toHaveCount(0);
  await page.screenshot({ path: "test-results/screens/settings-business.png" });
  await axeClean(page);
});

// Improvement wave 10: a recurring invoice is set up from the editor and
// paused from the list (skyla_app::recordings, "recurring").
test("a recurring invoice is set up, then paused", async ({ page }) => {
  await page.goto("/#/invoices/new");
  await settle(page);
  const form = page.getByRole("form", { name: "New invoice" });
  await form.getByRole("button", { name: /Customer/ }).click();
  await page.getByRole("option", { name: /Northwind Traders s\.r\.o\./ }).click();
  await form.getByLabel("Line 1 description").fill("Správa webu · {month}");
  await form.getByLabel("Line 1 quantity").fill("1");
  await form.getByLabel("Line 1 unit", { exact: true }).fill("ks");
  await form.getByLabel("Line 1 unit price").fill("6 000,00");
  await form.getByLabel("Note on the invoice").fill("Děkuji za spolupráci.");
  await form.getByRole("button", { name: /Repeat/ }).click();
  await page.getByRole("option", { name: "Every month" }).click();
  await form.getByLabel("First invoice on").fill("2026-11-01");
  await form.getByLabel("Template name").fill("Správa webu");
  await page.screenshot({ path: "test-results/screens/invoice-recurring.png" });
  await axeClean(page);
  await page.getByRole("button", { name: "Save template" }).click();
  await expect(page).toHaveURL(/#\/invoices$/);

  await page.getByRole("button", { name: /Recurring · \d/ }).click();
  const popup = page.getByRole("dialog", { name: "Recurring invoices" });
  const row = popup.getByRole("listitem").filter({ hasText: "Správa webu" });
  await expect(row).toContainText("next 1 Nov 2026");
  await row.getByRole("button", { name: "Pause" }).click();
  await expect(row).toContainText("paused");
});

test("inbox postings are approved through the kernel, one or the certain ones together", async ({
  page,
}) => {
  await page.goto("/#/inbox/p-alza");
  await settle(page);
  const inbox = page.getByRole("grid", { name: "Inbox" });
  const alza = page.getByRole("complementary", { name: /Alza\.cz/ });
  await expect(alza).toContainText("Proposed journal entry");
  await alza.getByRole("button", { name: "Approve and post" }).click();
  await expect(page.getByRole("status")).toContainText("Posted 1 entry.");
  await expect(inbox).not.toContainText("Alza.cz");

  await page.getByRole("button", { name: "Approve 2 certain" }).click();
  await expect(page.getByRole("status")).toContainText("Posted 2 entries.");
  await expect(inbox).not.toContainText("Google Ireland");
  await expect(inbox).not.toContainText("ČSOB");
  await expect(page.getByRole("button", { name: "Approve 0 certain" })).toBeDisabled();
  await axeClean(page);
  await page.screenshot({ path: "test-results/screens/inbox-approved.png" });

  // The lines are booked in the workbench, saying how.
  await page.goto("/#/bank/s1-4");
  await settle(page);
  await expect(page.getByRole("complementary", { name: /Alza/ })).toContainText(
    "Approved: Alza.cz",
  );
});

test("advice that has been read is dismissed from the inbox", async ({ page }) => {
  await page.goto(`/#/inbox/${encodeURIComponent("finding-duplicate:figma:42")}`);
  await settle(page);
  const figma = page.getByRole("complementary", { name: /Figma charged twice/ });
  await figma.getByRole("button", { name: "Dismiss" }).click();
  await expect(page.getByRole("status")).toContainText("Dismissed.");
  await expect(page.getByRole("grid", { name: "Inbox" })).not.toContainText("Figma");
});

test("one certain line is accepted, and its booking undone by a reversal", async ({ page }) => {
  await page.goto("/#/bank/s1-1");
  await settle(page);
  const pixelfarm = page.getByRole("complementary", { name: /Pixelfarm/ });
  await pixelfarm.getByRole("button", { name: "Accept", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("is booked: Pay PF-2026-0917");
  await expect(pixelfarm).toContainText("Booked");

  await pixelfarm.getByRole("button", { name: "Undo booking…" }).click();
  const undo = page.getByRole("dialog", { name: "Undo booking" });
  await expect(undo).toContainText("Posts a reversal of entry");
  await axeClean(page);
  await undo.getByRole("button", { name: "Post reversal" }).click();
  await expect(page.getByRole("status")).toContainText("is reversed");
  await expect(pixelfarm.getByRole("button", { name: "Accept", exact: true })).toBeVisible();
});
