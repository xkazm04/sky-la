import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

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
  build: { target: "es2022", sourcemap: true },
  // Playwright owns e2e/; Vitest runs unit tests only.
  test: { exclude: ["e2e/**", "node_modules/**"], passWithNoTests: true },
});
