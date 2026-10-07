import { defineConfig, devices } from "@playwright/test";

// Cloud sessions ship Chromium under PLAYWRIGHT_BROWSERS_PATH. If its build
// differs from the one this @playwright/test expects, point
// PLAYWRIGHT_CHROMIUM_EXECUTABLE at a Chromium binary instead.
const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;

export default defineConfig({
  testDir: "e2e",
  outputDir: "test-results",
  // Committed screenshot baselines (WP-10), one set per appearance. Update with `just e2e-update`.
  snapshotPathTemplate: "{testDir}/baseline/{arg}{ext}",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: "http://localhost:1420",
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        viewport: { width: 1360, height: 860 },
        launchOptions: { executablePath },
      },
    },
  ],
  webServer: {
    command: "pnpm dev:web",
    url: "http://localhost:1420",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
