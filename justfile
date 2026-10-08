# sky-la task runner. `just ci` is the gate every work packet must pass.

set shell := ["bash", "-euo", "pipefail", "-c"]

default: ci

# Everything a session or CI must keep green. Builds without WebKitGTK.
ci: fmt-check lint licence-check security-check test e2e

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

# CSP, capabilities, network paths and process spawns (WP-32).
security-check:
    python3 scripts/check_security.py

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
# On demand only: the advisor eval set through your own Claude Code CLI
# (uses your plan's usage; never part of `ci`). Writes target/evals/.
eval-live *args:
    cargo build -p skyla-mcp -p skyla-app --bin skyla-eval
    ./target/debug/skyla-eval --live {{args}}

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

# WP-17: fuzz every bank-statement parser (needs nightly and cargo-fuzz).
# Seeds from the samples and the committed corpus; `secs` per target.
fuzz secs="120":
    #!/usr/bin/env bash
    set -euo pipefail
    cd crates/skyla-bank
    # cargo-fuzz defaults to the target it was built for; a prebuilt musl
    # cargo-fuzz would then build for musl, which has no nightly std here.
    host=$(rustc +nightly -vV | sed -n 's/^host: //p')
    for t in parse_any camt053 mt940 gpc csv; do
      mkdir -p fuzz/corpus/$t
      cp -n tests/samples/* tests/corpus/$t/* fuzz/corpus/$t/ 2>/dev/null || true
      cargo +nightly fuzz run --target "$host" --debug-assertions $t -- -max_total_time={{secs}} -max_len=65536 -rss_limit_mb=2048
    done

# Supply-chain and licence policy for third-party crates.
deny:
    cargo deny check

# Builds the Linux .deb the way the release workflow does, collects it with
# SHA256SUMS and latest.json in dist-release/, and checks the sums (WP-33).
# Needs WebKitGTK. Unsigned: signing happens in CI with the maintainer's key.
release-dry-run target="x86_64-unknown-linux-gnu":
    cargo build --release --locked -p skyla-mcp --target {{target}}
    cargo run -q --locked -p skyla-release -- stage-shim {{target}}
    pnpm --filter @skyla/desktop tauri build --target {{target}} --bundles deb --config src-tauri/tauri.release.conf.json
    rm -rf dist-release
    cargo run -q --locked -p skyla-release -- collect target/{{target}}/release/bundle dist-release v$(cargo run -q --locked -p skyla-release -- version) $(date -u +%F)
    cargo run -q --locked -p skyla-release -- verify-sums dist-release
