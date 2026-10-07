# sky-la task runner. `just ci` is the gate every work packet must pass.

set shell := ["bash", "-euo", "pipefail", "-c"]

default: ci

# Everything a session or CI must keep green. Builds without WebKitGTK.
ci: fmt-check lint licence-check test e2e

# Format Rust and TypeScript in place.
fmt:
    cargo fmt --all
    pnpm exec biome check --write .

fmt-check:
    cargo fmt --all --check
    pnpm exec biome format .

lint:
    cargo clippy --all-targets --locked -- -D warnings
    pnpm exec biome ci .
    pnpm -r typecheck

licence-check:
    python3 scripts/check_licence_boundary.py

test:
    cargo test --locked
    pnpm -r test

# Playwright against the UI on the mock IPC transport (headless Chromium).
e2e:
    pnpm --filter @skyla/desktop e2e

# The UI in a plain browser on the fixture-backed mock transport.
dev-web:
    pnpm --filter @skyla/desktop dev:web

# The real desktop app (needs WebKitGTK on Linux; see docs/plan WP-00).
dev:
    pnpm --filter @skyla/desktop tauri dev

# Compile, lint and test the Tauri shell. Needs WebKitGTK on Linux.
check-desktop:
    cargo clippy -p skyla-desktop --all-targets --locked -- -D warnings
    cargo test -p skyla-desktop --locked

# Supply-chain and licence policy for third-party crates.
deny:
    cargo deny check
