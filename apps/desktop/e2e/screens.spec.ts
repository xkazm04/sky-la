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

        const results = await new AxeBuilder({ page })
          .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
          .analyze();
        expect(
          results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target).join(", ")}`),
        ).toEqual([]);

        const browser = page.context().browser()?.version() ?? "unknown";
        if (test.info().config.updateSnapshots === "all")
          writeFileSync(BROWSER_FILE, `${browser}\n`);
        if (browser === baselineBrowser || test.info().config.updateSnapshots === "all") {
          await expect(page).toHaveScreenshot(`${route}-${scheme}.png`, {
            maxDiffPixels: 50,
          });
        } else {
          test.info().annotations.push({
            type: "visual baseline",
            description: `made with Chromium ${baselineBrowser || "?"}; this run uses ${browser}, so pixels weren't compared`,
          });
          await page.screenshot({ path: `test-results/screens/${route}-${scheme}.png` });
        }
      });
    }
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
  const first = inbox.getByRole("row", { name: /Studio Brno → invoice 2026-102/ });
  await first.click();
  await expect(first).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("ArrowDown");
  await expect(page).toHaveURL(/#\/inbox\/p-google$/);
  await expect(page.getByRole("complementary", { name: /Google Ireland/ })).toContainText(
    "Proposed journal entry",
  );
  await expect(page.getByRole("complementary")).toContainText("242,11");

  await page.goto("/#/bank");
  const lines = page.getByRole("grid", { name: "Bank lines" });
  // Newest first: the ČSOB fee (6 Oct), then Alza (5 Oct).
  await lines.getByRole("row", { name: /ČSOB/ }).click();
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
  await expect(page.getByTestId("tie-out")).toContainText("Ties to the bank's reported closing");
  await expect(page.getByTestId("tie-out")).toContainText("902 741,58");
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
