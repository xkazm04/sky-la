# sky-la — Implementation Plan

| | |
|---|---|
| **Status** | v2, follows [`../design/REVIEW.md`](../design/REVIEW.md) |
| **Unit of work** | A **work packet (WP)**, sized to finish inside one Claude Code cloud session with green acceptance commands |
| **Progress** | Tracked in [`STATUS.md`](./STATUS.md) |

## How sessions use this plan

1. Read `CLAUDE.md`, then `STATUS.md`, then the next WP below.
2. Run the baseline (`just ci` once WP-00 exists). Never start a packet on a red baseline: fix the baseline first.
3. Build only what the packet lists. Anything else goes to `STATUS.md → Backlog` as a note.
4. A packet is **done** only when every acceptance command passes and `STATUS.md` is updated in the same commit series.
5. Commit messages are prefixed with the packet ID (`WP-05: enforce posted-entry immutability in SQL`).

Sizes: **S** ≈ a third of a session · **M** ≈ half to most of one · **L** ≈ a full session. If an L overruns, split it at a natural seam and record the split in `STATUS.md`.

## Milestones and critical path

```
M0 Foundation      WP-00 ─▶ WP-01 (spike) ─┐
                   WP-00 ─▶ WP-02 ─▶ WP-03 ─┤
M1 Ledger                    WP-04 ─▶ WP-05 ─▶ WP-06 ─▶ WP-07 ─┐
M2 Shell + DS      (dir. A) ─▶ WP-08 ─▶ WP-09 ─▶ WP-10 ──────────┤
M3 Invoicing                 WP-11 ─▶ WP-12 · WP-13 · WP-14 · WP-15 ─▶ WP-16
M4 Bank                      WP-17 ─▶ WP-18 ─▶ WP-19
M5 Tax CZ                    WP-20 ─▶ WP-21 · WP-22 · WP-23
M6 Advisors        WP-01 ─▶  WP-24 ─▶ WP-25 ─▶ WP-26 ─▶ WP-27 · WP-28 ─▶ WP-29
M7 Release                   WP-30 · WP-31 · WP-32 · WP-33 · WP-34
```

M2 can run in parallel with M1: design direction A is locked (D-011). M3–M5 need M1. M6 needs M1, M5 (for the scenario engine) and the spike's findings.

---

## M0 — Foundation

### WP-00 · Scaffold, CI, cloud-session readiness · **L**

**Goal:** a repository every later session can build, test and verify headlessly.

**Deliverables**
- Cargo workspace with empty crates per [`DESIGN.md` §3.2](../design/DESIGN.md). Rust 2024 edition; `rust-toolchain.toml` pinned; clippy config denying `float_arithmetic` in `skyla-money` and `skyla-ledger`.
- pnpm workspace: `apps/desktop` (Tauri v2 + React 19 + TS strict + Vite + Tailwind v4), `packages/ui`, `packages/ipc`, `packages/fixtures`.
- `packages/ipc`: a transport interface with a `mock` implementation, so `pnpm --filter desktop dev:web` runs the UI in a plain browser.
- `justfile`: `ci`, `test`, `lint`, `fmt`, `dev-web`, `e2e`.
- GitHub Actions: Rust fmt + clippy `-D warnings` + tests; `cargo-deny`; web lint, typecheck, Vitest; Playwright against `dev:web` on Chromium. Linux job required; macOS and Windows build jobs non-blocking.
- `.claude/` SessionStart hook installing the Rust toolchain, pnpm and dependencies, so cloud sessions can run `just ci` immediately.
- Tauri capabilities file with nothing granted beyond the defaults; CSP set (no remote sources, `connect-src 'none'` apart from IPC).
- Licensing per D-016: root `LICENSE` (AGPL-3.0-or-later); `LICENSE` (Apache-2.0) in `crates/skyla-money`, `crates/skyla-ledger` and `crates/skyla-rules`; SPDX `license` fields in every `Cargo.toml` and `package.json`; a `cargo-deny` rule that fails if an Apache-2.0 crate depends on an AGPL crate.

**Acceptance**
- `just ci` passes locally and in GitHub Actions.
- `just e2e` takes a Playwright screenshot of the placeholder home page in Chromium.
- A fresh cloud session runs `just ci` without manual setup.

**Out of scope:** any feature code.

### WP-01 · Claude Code CLI spike (timeboxed, code is throwaway) · **M**

**Goal:** turn every LLM-pillar assumption into a verified fact or a documented change.

**Deliverables**
- `spikes/cli/`: a minimal Rust binary that launches `claude` with the profile in [`REVIEW.md` §4.2](../design/REVIEW.md) and connects a toy stdio MCP server exposing `get_period_summary` and `propose_entry`. It parses stream-json and reads `structured_output`.
- `docs/spikes/WP-01-cli.md` with a ✓/✗ table for each item:
  - `--restricted` works with subscription sign-in.
  - `--tools ""` with MCP-only tools works.
  - What user context still loads (`~/.claude/CLAUDE.md`, auto memory, user hooks), and the flags that suppress it (`--setting-sources`, `--settings`).
  - `--json-schema` returns `structured_output`.
  - The `system/init` event lists only the sky-la MCP server.
  - Cold-start latency (p50 and p90 over 10 runs).
  - `total_cost_usd` is present for subscription and for API-key users.
  - Behaviour when `claude` isn't installed or isn't authenticated (exit codes, `result` error text).
  - Interrupt and SIGINT behaviour.
- Recorded transcripts in `crates/skyla-advisor/fixtures/` for the `Fake` driver.

**Acceptance:** the findings doc is committed. Every ✗ has a proposed design change, written into `DESIGN.md` or `DECISIONS.md`. Fixtures replay in a unit test.

**Notes:** run it on a machine where the developer is signed in to their *own* Claude Code. That's permitted use. If the cloud session has no authenticated CLI, commit the harness plus `scripts/spike-cli.sh` and the user runs it locally.

### WP-02 · `skyla-money` · **S**

**Deliverables**
- `Money { minor: i64, currency }` with checked arithmetic.
- `Rate` (decimal) and `RoundingMode` (half-even, half-up, toward-zero).
- `allocate(total, weights)`: the parts always sum to the total. Largest-remainder method, ties broken by weight then position, so it's deterministic.
- VAT helpers: base ↔ gross, with explicit rounding.
- `cs-CZ` formatting and parsing (`84 700,00`), plus ISO 4217 minor-unit table.

**Acceptance:** `cargo test -p skyla-money`. proptest checks: allocation sums to the total exactly; `gross(base(x))` round-trips within the declared tolerance; no panic on overflow, only an error.

### WP-03 · `skyla-store` + key management · **M**

**Deliverables**
- rusqlite with `bundled-sqlcipher-vendored-openssl`.
- Create, open and rekey.
- DEK/KEK scheme: Argon2id; `KeyStore` trait with `keyring` (OS keychain) and in-memory implementations; recovery-key generation and unwrap.
- Migrations framework.
- Single-writer actor with a read pool.
- Encrypted backup and restore through the SQLCipher backup API.

**Acceptance**
- A test proves the database file is unreadable without the key (opening it with plain SQLite fails).
- Rekey round-trip.
- A database unlocks with the recovery key after the passphrase is "forgotten".
- Backup → restore → identical content hash.

---

## M1 — Ledger kernel

### WP-04 · Accounts, chart of accounts, categories · **M**

- Schema for `account`, `period` and `category`.
- The Czech *směrná účtová osnova* seed (the classes needed for P1 and P2).
- A P1 category layer ("Software and subscriptions" → 518; "Small equipment" → 501, …).
- Leaf-only posting rule data.

**Acceptance:** seeds load; every category maps to a leaf account; snapshot test of the seeded chart.

### WP-05 · Posting engine + invariants I1–I7 · **L**

- `journal_entry` and `posting` with dimensions.
- Draft → posted transition.
- SQL triggers for I1–I4 (balance at post, immutability, closed periods, leaf accounts) plus Rust validation with typed errors.
- `approved_by` required for rule- and advisor-sourced entries (I7).

**Acceptance (property tests):**
- Any random *balanced* entry posts.
- Any random *unbalanced* entry is rejected by Rust *and* by raw SQL bypassing Rust.
- `UPDATE` and `DELETE` on posted rows abort even through raw SQL.
- Posting into a closed period aborts.
- 10 000-entry fuzz run with zero invariant violations.

### WP-06 · Periods, reversals, close checks · **M**

- Period state machine (open → closing → closed).
- Reversal entries linked through `reverses_id`.
- A close-check framework (pluggable checks: journal balanced, bank tied out, VAT ledger matches account 343, no draft documents).
- **Hash chain on by default** (D-007): `chain_hash` computed in the posting transaction; verifier on open and export.

**Acceptance:** reversal leaves balances at zero; the close blocks while a check fails; the chain verifier detects a single tampered byte and names the first bad entry.

### WP-07 · Projections · **L**

- Trial balance, P&L, balance sheet.
- **Cash basis (*daňová evidence*)**, using settlement links and `tax_treatment`.
- VAT ledger by form row.
- An input-snapshot hash on every report.

**Acceptance**
- Golden datasets: the design-canvas entity (`packages/fixtures`) reproduces the Q3 P&L shown in direction C to the cent.
- Cash-basis income for the Northwind receipt equals the base of 70 000,00.
- Trial balance over 100k entries runs in < 200 ms (bench).

---

## M2 — Shell and design system *(direction A locked; see `DESIGN.md` §6)*

### WP-08 · Tokens + primitives · **L**
Tokens for **direction A (Tahoe)**: light, plus dark if Q-09 confirms it (colour, type scale, spacing, radii, elevation, glass materials with a non-blur fallback). React Aria-based primitives: Button, SegmentedControl, SearchField, Popup, Menu, Table/Grid, Inspector, SourceList, StatusBar, Badge, Kbd. **Acceptance:** Storybook-free gallery route in `dev:web`; Playwright screenshots in both appearances; axe accessibility check with no violations.

### WP-09 · Typed IPC + fixtures · **M**
`tauri-specta`-generated bindings (version pinned) behind `packages/ipc`. The mock transport serves `packages/fixtures`: the canvas entity with its invoices, bank lines, entries and proposals. **Acceptance:** one command round-trips identically over the mock and the real transport (integration test).

### WP-10 · Navigable app on fixtures · **L**
Every v1 screen reachable with fixture data in A's three-pane chrome: overview, **inbox** (B pattern), invoices, bank workbench, statements, taxes, advisors, settings and the egress register. **Acceptance:** Playwright walks every route; screenshot baseline committed; keyboard navigation works for the main lists.

---

## M3 — Invoicing

| WP | Scope | Size | Acceptance highlights |
|---|---|---|---|
| WP-11 | Documents, series with gap detection, lifecycle, posting AR + VAT, credit notes, advance invoices + tax document on received payment | L | Lifecycle property tests; issued documents immutable; credit note restores the balance |
| WP-12 | Typst templates (cs/en) → PDF; SPAYD QR Platba | M | PDF snapshot (text layer); SPAYD string matches the spec examples; QR decodes back to the same SPAYD |
| WP-13 | ISDOC 6 writer | M | XSD-valid; snapshot tests |
| WP-14 | EN 16931: UBL 2.1 (Peppol BIS Billing 3.0) + CII writers | L | XSD-valid; **KoSIT validator container in CI**: zero Schematron errors on the golden set |
| WP-15 | Recurring templates; dunning sequences; statutory late interest from the pack | M | Schedule tests over a simulated year; interest matches hand-computed cases |
| WP-16 | Invoicing UI (direction A list and inspector or its equivalent) | L | e2e: create → issue → export PDF/ISDOC on the mock |

## M4 — Bank

| WP | Scope | Size | Acceptance highlights |
|---|---|---|---|
| WP-17 | Parsers: CAMT.053, MT940, ABO/GPC, CSV with profiles | L | Real-world sample corpus (anonymised); `cargo-fuzz` targets with CI corpus replay; zero panics |
| WP-18 | Normalise, dedupe, statement tie-out, user rules, explainable scorer, splits | L | Tie-out fails loudly on a missing line; scorer contributions sum to the score; ≥ 85 % auto-match on the fixture year |
| WP-19 | Reconciliation workbench UI + inbox items | L | e2e: import → accept certain matches → split one → create a rule |

## M5 — Tax CZ

| WP | Scope | Size | Acceptance highlights |
|---|---|---|---|
| WP-20 | `cz-2026` pack data, effective dating, citations, loader; **opt-in reference data** (ČNB FX fetch, off by default, with manual import; minisign-verified pack updates) per D-006 | L | Every value has a citation; schema validation of the pack; a tampered pack signature is rejected; the app works with fetching disabled |
| WP-21 | DPH return, kontrolní hlášení, souhrnné hlášení: computation + EPO XML (format versions pinned in the pack) | L | XSD-valid against the current EPO schemas; golden cases incl. reverse charge, the 12 % rate, non-deductible items, the KH threshold split |
| WP-22 | DPFO §7 worksheet; **scenario engine** with levers (flat-rate vs actual, *paušální daň* eligibility, asset threshold timing) | L | Golden cases hand-verified. The flat-rate example in the review (projected 1 571 000 → 942 600 vs 383 200) reproduces exactly, side effects included |
| WP-23 | Obligations calendar; insurance overview data | M | Calendar generated from the pack for 2026; deadline-shift rules (weekends and holidays) tested |

## M6 — Advisors

| WP | Scope | Size | Acceptance highlights |
|---|---|---|---|
| WP-24 | `LlmProvider` trait; production `ClaudeCodeCli` driver (from WP-01 findings); `Fake` driver | M | Driver contract tests on recorded transcripts; clear UX states for *not installed*, *not signed in* and *rate-limited* |
| WP-25 | `skyla-mcp` stdio shim + authenticated local socket to the core; read/compute/propose tool surface | L | No tool can write or post (tests); the shim has no database access (process test); token rotation per run |
| WP-26 | Egress gate (scopes, policies, pseudonymisation), encrypted register, "What was shared" UI | L | Tool results pass through the gate (test); IBANs and personal IDs never appear in any payload (property test); register replay is byte-identical |
| WP-27 | Tax advisor: lever selection, `ask_user`, explanation, **numeric-grounding validator** | L | A run with an injected wrong number is rejected; the eval set (≥ 30 CZ cases) passes on `Fake` |
| WP-28 | Financial advisor detectors (variance, vendor rate change, margin by client, late payer, subscription creep, runway) + **inline "explain this"** on any figure, account or line (D-015) | L | Detector unit tests on fixtures (the 42 % subcontracting finding in direction C reproduces); explain-this cites entry IDs |
| WP-29 | `AnthropicApi` driver (raw HTTPS, structured outputs, explicit effort, refusal fallbacks) + live eval harness | M | On-demand eval run with recorded cost; never runs in default CI |

## M7 — Hardening and release

| WP | Scope | Size |
|---|---|---|
| WP-30 | Backup scheduling, recovery-key UX, restore drill; **OS keychain `KeyStore` adapter** (keyring 4.x: macOS Keychain, Windows Credential Manager, Secret Service), moved from WP-03 | M |
| WP-31 | Import from Pohoda and Fakturoid exports; full export of journal JSON + CSV and the document archive | L |
| WP-32 | Security review: threat-model tests, CSP and capability audit, supply-chain gates | M |
| WP-33 | Packaging: macOS notarisation, Windows signing, Linux AppImage/deb; opt-in update channel | M |
| WP-34 | User docs; rule-pack contributor kit (template, golden-test harness, citation rules) | M |

## Backlog (post-v1)

Written period review (v1.x) · EU OSS · UK and US community packs · Peppol Access Point integration · Factur-X/ZUGFeRD hybrid PDF · LLM bill capture · scoped chat advisor · multi-entity · encrypted sync · payroll · bank API connectors (e.g. Fio token API) · WASM sandboxed packs.
