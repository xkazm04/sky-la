#!/bin/bash
# Prepares a Claude Code cloud session so `just ci` runs immediately.
# Idempotent and non-interactive. WebKitGTK is not installed here: only
# `just check-desktop` needs it, and CI covers that job.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(dirname "$0")/../..}"

# Rust: rustup installs the toolchain pinned in rust-toolchain.toml on first use.
rustup show active-toolchain >/dev/null
command -v just >/dev/null 2>&1 || cargo install just --locked --quiet
cargo fetch --locked

# JavaScript dependencies (`install`, not a frozen install, so the cached container stays warm).
corepack enable >/dev/null 2>&1 || true
pnpm install --prefer-offline

# Playwright: use the container's Chromium when the build this @playwright/test
# expects isn't installed (browser downloads are skipped in cloud sessions).
if [ -n "${CLAUDE_ENV_FILE:-}" ] && [ -x /opt/pw-browsers/chromium ]; then
  expected=$(node -e '
    const fs = require("fs"), path = require("path");
    const pw = require.resolve("playwright-core/package.json", { paths: [require.resolve("@playwright/test/package.json", { paths: ["apps/desktop"] })] });
    const b = JSON.parse(fs.readFileSync(path.join(path.dirname(pw), "browsers.json"))).browsers.find(x => x.name === "chromium");
    process.stdout.write(b ? b.revision : "");
  ' 2>/dev/null || true)
  if [ -z "$expected" ] || [ ! -d "${PLAYWRIGHT_BROWSERS_PATH:-/opt/pw-browsers}/chromium-$expected" ]; then
    echo "export PLAYWRIGHT_CHROMIUM_EXECUTABLE=/opt/pw-browsers/chromium" >> "$CLAUDE_ENV_FILE"
  fi
fi
