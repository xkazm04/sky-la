# sky-la — Status

Update this file in the same commit series that completes a work packet.

## Current state

- **Phase:** M1 Ledger kernel in progress. Done: WP-00 – WP-07 (M0 and M1 complete).
- **Next packet:** WP-08 (direction A tokens + primitives, light and dark; D-017). M2 can start now that the kernel is complete.
- **Blocking decisions:** none open. Wave 4 recorded as D-017 (dark first-class from WP-08) and D-018 (spike run in the cloud).

## Packets

| WP | Title | Status | Session / commit | Notes |
|---|---|---|---|---|
| WP-00 | Scaffold, CI, cloud readiness | done | this session | `just ci` green locally; GitHub Actions runs on push |
| WP-01 | Claude Code CLI spike | done | this session | 10 of 11 checks pass; C6 (signed out, desktop keychain) needs a real machine. Findings in `docs/spikes/WP-01-cli.md` |
| WP-02 | skyla-money | done | this session | 30 tests incl. 8 property tests; float ban verified |
| WP-03 | skyla-store + keys | done | this session | 13 tests; OS keychain adapter moved to WP-30 |
| WP-04 | Accounts + categories | done | this session | 7 tests + 2 snapshots; CZ chart in `rules/cz/chart.toml` |
| WP-05 | Posting engine + invariants | done | this session | 11 tests incl. property tests and a 10 000-entry fuzz run |
| WP-06 | Periods, reversals, close | done | this session | 10 tests incl. 40 random single-byte tamperings; chain head sealed at close |
| WP-07 | Projections | done | this session | 7 tests on the golden journal + `just bench` (TB over 100k entries: ~0.1 s) |
| WP-08 | Tokens + primitives (direction A) | todo | | Dark first-class (D-017) |
| WP-09 | Typed IPC + fixtures | todo | | |
| WP-10 | Navigable app on fixtures | todo | | |
| WP-11 – WP-34 | See the plan | todo | | |

## Log

- 2026-10-07 — Design round 1 (six dashboard skins) rejected. Full solution review completed; four round-2 directions published to the design canvas. `DESIGN.md`, `REVIEW.md`, `DECISIONS.md` and this plan written.
- 2026-10-07 — Wave 3: direction A (Tahoe) locked. CZ-first, API-key driver, hash chain on and opt-in reference data accepted. Advisors reached through the inbox + explain-this. Licence: AGPL app + Apache-2.0 engine crates.
- 2026-10-07 — WP-00: Cargo workspace (11 engine crates + Tauri shell), pnpm workspace (desktop, ipc, fixtures, ui), Biome, `justfile`, CI (core, cargo-deny, desktop Linux, desktop macOS/Windows non-blocking), licence files and boundary check, SessionStart hook. Resolved versions: Rust 1.97, TypeScript 7, Vite 8, Vitest 5, Playwright 1.63, Tailwind 4.3, Tauri 2.
- 2026-10-07 — WP-02: `Money` (i64 minor units + ISO 4217 table), checked arithmetic, `RoundingMode` (half-even, half-up, toward zero, away from zero), FX `convert`, exact `allocate`, `vat::{from_base, from_gross}` with base + VAT = gross guaranteed, strict cs-CZ format/parse. Canvas figures reproduced: 84 700 → 70 000 + 14 700; 490 € × 25,140 = 12 318,60; reverse-charge VAT 2 586,91.
- 2026-10-07 — WP-03: SQLCipher store (vendored OpenSSL) with a single writer thread, read connection, migrations (gapless versions, refuses newer schemas), rekey, encrypted backup via `sqlcipher_export` and validated restore, order-independent content hash. Vault: random 256-bit data key wrapped under Argon2id(passphrase) and a printable recovery key with XChaCha20-Poly1305, purpose-bound AAD, key-check value, atomic save. Constant-time key equality; keys zeroised on drop.
- 2026-10-07 — CI: desktop jobs and cargo-deny fixed (crate docs; web build before Tauri; scoped RUSTSEC-2024-0370 exception).
- 2026-10-07 — WP-04: jurisdiction-neutral `ChartSpec` (TOML) with a validator that reports every problem at once; CZ chart (63 synthetic accounts per vyhláška 500/2002 Sb., 22 freelancer categories with default tax treatment) as cited rule-pack data; ledger schema (`SCHEMA`) with triggers enforcing account structure (three-digit tops, analytic codes extend parents, kind/side inherited, structure immutable, never deleted, no sub-accounts under category targets) and periods (valid ISO dates, no overlap, fixed dates). The ledger owns its SQL and uses plain `rusqlite`, keeping the Apache-2.0 crate free of the AGPL store.
- 2026-10-07 — CI: Windows desktop job fixed (generated `icon.ico` and the desktop icon set).
- 2026-10-07 — WP-05: journal schema and posting engine. Invariants enforced in Rust (typed errors) and again in triggers (raw SQL can't bypass them): I1 balanced in functional currency, I2 posted entries and postings frozen and never deleted, I3 an open period covers the date, I4 active leaf accounts (re-checked at post time, which found and closed a gap where an account deactivated after drafting could still be posted to), I7 human approver for rule/advisor entries; gapless `posted_seq`; FX lines must carry their rate and same-sign functional amount; functional currency fixed once anything is booked; savepoint-based operations nest inside a caller's transaction.
- 2026-10-07 — WP-01: CLI spike run live in this cloud session (cheapest model, ~20 short calls). The hardened profile works with OAuth sign-in, keeps `~/.claude/CLAUDE.md` out (canary test), exposes only sky-la MCP tools plus `StructuredOutput`, and returns `structured_output` and a cost estimate. Added to the profile: `--disable-slash-commands`, stdin closed (an open stdin costs a 3 s wait; p50 now ≈ 1.35 s), a scrubbed environment with `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` (the child otherwise inherits the parent's session id). `skyla_advisor::cli_stream` parses stream-json and guards the profile; 6 replay tests over 4 recorded transcripts.
- 2026-10-07 — WP-06: period state machine (open → closing → closed; closing takes adjustments; closed is final; date order; close records who, when and the chain head), pluggable `CloseCheck`s with four kernel checks and a report that lists every failure, `reverse_entry` (mirror entry via `reverses_id`, at most once, foreign amounts included), and the SHA-256 hash chain computed in the posting transaction. `verify_chain` names the first missing, altered or seal-inconsistent entry. All rules also hold as triggers against raw SQL.
- 2026-10-07 — WP-07: trial balance, P&L and balance sheet (by synthetic account), cash basis (*daňová evidence*) over settlement links and cash-flagged accounts, VAT ledger by form row from pack-supplied rules, and an input-snapshot hash on every report. Golden journal `packages/fixtures/data/demo-ledger.json` (52 entries for Jan Novák, Q2–Q3 2026) reproduces the canvas P&L to the cent: Q3 revenue 456 500,00, expenses 176 150,00, profit 280 350,00 (Q2: 391 000,00 / 112 220,00 / 278 780,00). Northwind receipt → cash-basis income 70 000,00. Trial balance over 100 000 entries: ~106 ms in release (one pass over covering indexes; the first two-query version took 426 ms).

## Backlog notes from sessions

- Playwright 1.63 expects Chromium build 1243; cloud containers ship 1194. Handled through `PLAYWRIGHT_CHROMIUM_EXECUTABLE`. Revisit if the container image updates.
- `tauri-specta` is still a release candidate; WP-09 pins it.
- WP-03 scope split: the OS keychain `KeyStore` adapter moved to WP-30. keyring 4.x has a new store-based API, and the adapter belongs with the unlock UX. `MemoryKeyStore` covers tests and headless sessions.
- One entity per encrypted database file (no `entity` table). Isolation between entities is then physical, and a bookkeeper's multi-entity view (post-v1) opens several stores.
- SQLCipher logs `error decrypting page 1` to stderr on a wrong-key attempt. That's expected in the wrong-key tests; consider `PRAGMA cipher_log_level` when the app gets structured logging.
- **C6 on a desktop before WP-24:** run `spikes/cli/run_spike.sh` signed out, and signed in through the macOS keychain, Linux and Windows. It settles the scrubbed-environment allow-list (e.g. `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`) and the not-signed-in error shape.
- The chain detects edits, deletions and (after a close) truncation, but not a whole rebuilt chain by someone who also rewrites the seals. Anchoring the head outside the database (in each encrypted backup's manifest and on export) closes that; do it with WP-30/WP-31.
- Reversing into an account deactivated since the original was posted fails on I4. The UI should offer to reactivate it (WP-10).
- The canvas groups "538 · 568 · 549"; the synthetic CZ chart has 548, not 549. The golden data uses 548. Add 549 to the chart if the rule pack (WP-20) wants it.
- WP-09 should serve `packages/fixtures/data/demo-ledger.json` through the mock transport, so the UI and the kernel tests share one dataset.
- Cash basis treats FX differences on settlement (563/663) as ordinary revenue and expense lines of the payment. Check against *daňová evidence* practice in WP-22.
- A reversed invoice that was already settled still recognises income through its links; credit notes (WP-11) must reverse links too.
