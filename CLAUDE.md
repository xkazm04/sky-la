# sky-la

Open-source, local-first, encrypted desktop accounting for Czech freelancers (OSVČ) and micro-companies. Tauri v2 with a Rust core and a React 19 + TypeScript + Tailwind v4 frontend. AI advisors run through the user's own Claude Code CLI and **propose, explain and calculate; they never post**.

## Read first

1. `docs/plan/STATUS.md`: where the project is and which work packet is next.
2. `docs/plan/IMPLEMENTATION_PLAN.md`: the packet's deliverables and acceptance commands.
3. `docs/design/DESIGN.md` for architecture; `docs/design/REVIEW.md` for the reasons behind it; `docs/design/DECISIONS.md` for what's locked and what's open.

## Invariants (never break, never weaken a test that guards one)

- **Money is integer minor units** (`skyla-money::Money`). No floats in money or ledger paths; clippy denies `float_arithmetic` there.
- **Posted journal entries are immutable.** Corrections are reversal entries. Enforced in SQL triggers *and* Rust.
- **Every posted entry balances** in functional currency. No posting into closed periods. Postings target leaf accounts only.
- **The LLM never writes to the ledger.** Advisor and rule output is a *proposal* that the kernel validates and a human approves.
- **All tax and accounting arithmetic lives in Rust** (engine and rule packs). Numbers in advisor text must match engine values (numeric-grounding check).
- **Statutory values come from rule packs** (`rules/<cc>/<year>/pack.toml`) with citations. Never hard-code a rate, threshold or deadline in logic.
- **The webview holds no secrets and enforces no rules.** The Rust core re-validates every command.
- **No network from the app** except (1) the user's own `claude` process, (2) opt-in signed public reference data, (3) an opt-in update check. No telemetry.
- **Egress:** prompts *and MCP tool results* pass through the egress gate. IBANs and personal IDs are never sent. Every run is recorded in the register.
- **Claude Code usage terms:** use only the user-installed, unmodified `claude` binary. Never read, store or proxy Claude credentials; sign-in happens via `claude auth login` in the user's terminal. Never intermediate billing. Don't use Claude, Claude Code or Anthropic names or logos in product or feature names.
- **Advisors are not tax advice.** UI copy says scenario, draft, for your review.
- **Licence boundary:** `skyla-money`, `skyla-ledger` and `skyla-rules` are Apache-2.0 and must never depend on AGPL crates; everything else is AGPL-3.0-or-later.
- **Design language is direction A (Tahoe)**, specified in `docs/design/DESIGN.md` §6. Don't introduce display serifs, decorative monospace or bordered card grids.

## Working in cloud sessions

- Work one packet at a time. Done means every acceptance command for the packet passes and `docs/plan/STATUS.md` is updated.
- Commit messages start with the packet ID: `WP-05: …`.
- The UI must run headlessly: `dev:web` uses the mock IPC transport and `packages/fixtures`. Verify UI work with Playwright screenshots on Chromium.
- Advisor work uses the `Fake` provider and the recorded transcripts in `crates/skyla-advisor/fixtures/`. Live model runs happen only on explicit request.
- Out-of-scope ideas go to `STATUS.md → Backlog notes`, not into the diff.

## Commands

```
just ci             # the gate: fmt check, clippy -D warnings, biome, typecheck, licence boundary, security audit, tests, Playwright e2e
just test           # Rust + web unit tests
just dev-web        # UI in a plain browser on the fixture-backed mock transport (http://localhost:1420)
just e2e            # Playwright against dev-web; screenshots land in apps/desktop/test-results/
just e2e-update     # regenerate the committed screenshot baselines after an intended UI change
just bench          # release-build performance acceptance (WP-07: trial balance over 100k entries < 200 ms)
just recordings     # re-record the core's IPC answers that the mock transport replays (after changing core output)
just bindings       # regenerate packages/ipc/src/bindings.ts from the Tauri commands (needs WebKitGTK)
just check-desktop  # compile + lint + test the Tauri shell (needs WebKitGTK on Linux; CI runs it)
just deny           # cargo-deny supply-chain and licence policy (CI runs it)
just security-check # CSP, capabilities, network paths and process spawns (part of `just ci`; see docs/design/SECURITY_REVIEW.md)
just fmt            # format Rust and TypeScript in place
```

- `.claude/hooks/session-start.sh` prepares cloud sessions (toolchain, `just`, crates, pnpm). It doesn't install WebKitGTK, so `check-desktop` isn't part of `just ci`.
- If the container's Chromium build differs from the one `@playwright/test` expects, the hook exports `PLAYWRIGHT_CHROMIUM_EXECUTABLE=/opt/pw-browsers/chromium`; `playwright.config.ts` honours it.
- Default cargo members exclude `apps/desktop/src-tauri`. Plain `cargo test` / `cargo clippy` never need WebKitGTK.
- The webview talks to the core only through the generated `commands` in `packages/ipc/src/bindings.ts`. Outside Tauri, `connectCore()` mocks the IPC layer to replay `ipc-recordings.json`, so `dev:web` shows exactly what the Rust core returns. A new command needs a `Core` method, a Tauri command, a canonical request in `skyla_app::recordings`, then `just recordings` and `just bindings`.
