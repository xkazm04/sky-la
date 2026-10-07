import { expect, test } from "@playwright/test";

test("the shell renders and reaches the core through the mock transport", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("navigation", { name: "Sections" })).toContainText("sky-la");
  await expect(page.getByTestId("status-line")).toHaveText(/sky-la 0\.1\.0 · core via mock/);
  await page.screenshot({ path: "test-results/screenshots/shell.png", fullPage: true });
});
