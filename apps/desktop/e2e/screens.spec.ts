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
  { route: "bank", title: "221 · ČSOB Business ··4412" },
  { route: "statements", title: "Profit and loss" },
  { route: "taxes", title: "Taxes" },
  { route: "advisors", title: "Advisors" },
  { route: "register", title: "Egress register" },
  { route: "settings", title: "Settings" },
] as const;

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
