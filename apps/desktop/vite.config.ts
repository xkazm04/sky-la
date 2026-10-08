import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// The Tauri CLI sets TAURI_ENV_PLATFORM when it builds the desktop bundle.
const desktop = Boolean(process.env.TAURI_ENV_PLATFORM);

// Port 1420 is what tauri.conf.json's devUrl expects.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Playwright writes baselines and results here; a reload mid-test would break it.
    watch: { ignored: ["**/src-tauri/**", "**/e2e/**", "**/test-results/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  // The desktop bundle talks to the Rust core, so it ships neither the mock
  // transport's recordings nor source maps; dev:web and e2e keep both.
  build: { target: "es2022", sourcemap: !desktop },
  resolve: desktop
    ? {
        alias: {
          "@skyla/fixtures/ipc-recordings.json": fileURLToPath(
            new URL("./src/no-recordings.json", import.meta.url),
          ),
        },
      }
    : {},
  // Playwright owns e2e/; Vitest runs unit tests only.
  test: { exclude: ["e2e/**", "node_modules/**"], passWithNoTests: true },
});
