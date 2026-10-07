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

# Regenerate the committed screenshot baselines (apps/desktop/e2e/baseline) after an intended UI change.
e2e-update:
    pnpm --filter @skyla/desktop exec playwright test screens --update-snapshots=all

# The UI in a plain browser on the fixture-backed mock transport.
dev-web:
    pnpm --filter @skyla/desktop dev:web

# The real desktop app (needs WebKitGTK on Linux; see docs/plan WP-00).
dev:
    pnpm --filter @skyla/desktop tauri dev

# Compile, lint and test the Tauri shell. Needs WebKitGTK on Linux.
# Builds the web app first: a non-dev Tauri build embeds `dist/`.
check-desktop:
    pnpm --filter @skyla/desktop build
    cargo clippy -p skyla-desktop --all-targets --locked -- -D warnings
    cargo test -p skyla-desktop --locked

# Performance acceptance (release build): WP-07 trial balance over 100 000 entries < 200 ms.
bench:
    cargo test -p skyla-ledger --release --locked --test projections -- --ignored --nocapture

# Re-record the core's answers the mock transport replays (packages/fixtures/data/ipc-recordings.json).
recordings:
    UPDATE_RECORDINGS=1 cargo test -p skyla-app --test core the_committed_recordings_match_the_core

# Regenerate packages/ipc/src/bindings.ts from the Tauri commands (needs WebKitGTK on Linux).
bindings:
    UPDATE_BINDINGS=1 cargo test -p skyla-desktop --test ipc the_generated_bindings_are_current

# WP-14 acceptance: write the golden e-invoices, then check each against its
# XSD and the official Schematron (CEN EN 16931, OpenPeppol BIS Billing 3.0).
# Fetches pinned validator artefacts from Maven Central once (needs Java 17+).
einvoice:
    rm -rf target/einvoice-golden
    SKYLA_EINVOICE_OUT="$PWD/target/einvoice-golden" cargo test -p skyla-invoicing --locked --test einvoice
    scripts/einvoice/validate.sh target/einvoice-golden/*.xml

# Supply-chain and licence policy for third-party crates.
deny:
    cargo deny check
