# sky-la — Status

Update this file in the same commit series that completes a work packet.

## Current state

- **Phase:** M0 Foundation. WP-00 and WP-02 done.
- **Next packet:** WP-03 (`skyla-store` + keys). WP-01 (CLI spike) waits for the go-ahead because it spends live Claude usage.
- **Blocking decisions:** none. Open: Q-08 (next session's focus) and Q-09 (dark appearance) in [`../design/DECISIONS.md`](../design/DECISIONS.md).

## Packets

| WP | Title | Status | Session / commit | Notes |
|---|---|---|---|---|
| WP-00 | Scaffold, CI, cloud readiness | done | this session | `just ci` green locally; GitHub Actions runs on push |
| WP-01 | Claude Code CLI spike | todo | | |
| WP-02 | skyla-money | done | this session | 30 tests incl. 8 property tests; float ban verified |
| WP-03 | skyla-store + keys | todo | | |
| WP-04 | Accounts + categories | todo | | |
| WP-05 | Posting engine + invariants | todo | | |
| WP-06 | Periods, reversals, close | todo | | |
| WP-07 | Projections | todo | | |
| WP-08 | Tokens + primitives (direction A) | todo | | Q-09 decides dark |
| WP-09 | Typed IPC + fixtures | todo | | |
| WP-10 | Navigable app on fixtures | todo | | |
| WP-11 – WP-34 | See the plan | todo | | |

## Log

- 2026-10-07 — Design round 1 (six dashboard skins) rejected. Full solution review completed; four round-2 directions published to the design canvas. `DESIGN.md`, `REVIEW.md`, `DECISIONS.md` and this plan written.
- 2026-10-07 — Wave 3: direction A (Tahoe) locked. CZ-first, API-key driver, hash chain on and opt-in reference data accepted. Advisors reached through the inbox + explain-this. Licence: AGPL app + Apache-2.0 engine crates.
- 2026-10-07 — WP-00: Cargo workspace (11 engine crates + Tauri shell), pnpm workspace (desktop, ipc, fixtures, ui), Biome, `justfile`, CI (core, cargo-deny, desktop Linux, desktop macOS/Windows non-blocking), licence files and boundary check, SessionStart hook. Resolved versions: Rust 1.97, TypeScript 7, Vite 8, Vitest 5, Playwright 1.63, Tailwind 4.3, Tauri 2.
- 2026-10-07 — WP-02: `Money` (i64 minor units + ISO 4217 table), checked arithmetic, `RoundingMode` (half-even, half-up, toward zero, away from zero), FX `convert`, exact `allocate`, `vat::{from_base, from_gross}` with base + VAT = gross guaranteed, strict cs-CZ format/parse. Canvas figures reproduced: 84 700 → 70 000 + 14 700; 490 € × 25,140 = 12 318,60; reverse-charge VAT 2 586,91.

## Backlog notes from sessions

- Playwright 1.63 expects Chromium build 1243; cloud containers ship 1194. Handled through `PLAYWRIGHT_CHROMIUM_EXECUTABLE`. Revisit if the container image updates.
- `tauri-specta` is still a release candidate; WP-09 pins it.
