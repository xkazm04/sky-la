import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

// WP-08 acceptance: the gallery in both appearances, with no axe violations.
for (const scheme of ["light", "dark"] as const) {
  test.describe(`${scheme} appearance`, () => {
    test.use({ colorScheme: scheme });

    test("the gallery renders every primitive and passes axe", async ({ page }) => {
      await page.goto("/#/gallery");
      await expect(page.getByRole("heading", { name: "Design gallery" })).toBeVisible();
      await expect(page.locator("html")).toHaveAttribute("data-appearance", scheme);
      await expect(page.getByRole("grid", { name: "Invoices" })).toBeVisible();
      await page.evaluate(() => document.fonts.ready);

      const results = await new AxeBuilder({ page })
        .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
        .analyze();
      expect(
        results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target).join(", ")}`),
      ).toEqual([]);

      await page.screenshot({
        path: `test-results/screenshots/gallery-${scheme}.png`,
        fullPage: true,
      });
      await page.getByTestId("composition").screenshot({
        path: `test-results/screenshots/window-${scheme}.png`,
      });
    });
  });
}

test("open popovers and menus pass axe too", async ({ page }) => {
  await page.goto("/#/gallery");
  await page.getByRole("button", { name: "More actions" }).click();
  await expect(page.getByRole("menu", { name: "More actions" })).toBeVisible();
  const results = await new AxeBuilder({ page }).include('[role="menu"]').analyze();
  expect(results.violations).toEqual([]);
  await page.screenshot({ path: "test-results/screenshots/menu-open.png" });
});

test("the primitives work from the keyboard", async ({ page }) => {
  await page.goto("/#/gallery");

  // Appearance switch: arrow keys move between segments, Enter/Space selects.
  const light = page.getByRole("radio", { name: "Light" });
  await page.getByRole("radio", { name: "System" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(light).toBeFocused();
  await page.keyboard.press("Space");
  await expect(page.locator("html")).toHaveAttribute("data-appearance", "light");
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Space");
  await expect(page.locator("html")).toHaveAttribute("data-appearance", "dark");

  // Source list: arrows move the selection.
  const sections = page.getByRole("listbox", { name: "Sections" });
  await sections.getByRole("option", { name: "Invoices" }).focus();
  await page.keyboard.press("ArrowDown");
  await expect(sections.getByRole("option", { name: "Bank" })).toHaveAttribute(
    "aria-selected",
    "true",
  );

  // Table: arrows move the selection and the inspector follows; section rows are skipped.
  const table = page.getByRole("grid", { name: "Invoices" });
  await table.getByRole("row", { name: /2026-114/ }).click();
  await page.keyboard.press("ArrowUp");
  await expect(page.getByRole("complementary", { name: "Invoice 2026-122" })).toBeVisible();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("complementary", { name: "Invoice 2026-097" })).toBeVisible();

  // Menu: Enter opens, arrows move, Escape closes and returns focus.
  const more = page.getByRole("button", { name: "More actions" });
  await more.focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("menuitem", { name: /Duplicate/ })).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("menuitem", { name: /Export ISDOC/ })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).toBeHidden();
  await expect(more).toBeFocused();

  // Search: typing shows the clear button; Escape clears.
  const search = page.getByRole("searchbox", { name: "Search invoices" });
  await search.fill("north");
  await page.keyboard.press("Escape");
  await expect(search).toHaveValue("");
});
