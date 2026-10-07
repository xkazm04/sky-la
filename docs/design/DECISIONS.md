# sky-la — Decision Log

Each decision records what was asked, what was chosen, and its current status.
Status values: **Locked** · **Open**.

## Decisions

| ID | Decision | Source | Status |
|---|---|---|---|
| D-001 | Brand name **sky-la** | Brief | Locked |
| D-002 | Desktop app on **Tauri v2**; **open source**; **React + TypeScript + Tailwind** client | Brief | Locked |
| D-003 | v1 personas: **freelancer (OSVČ)** and **small business**, one entity each. Payroll is imported, not calculated | Wave 1 + review | Locked |
| D-004 | Jurisdictions: the **CZ pack ships complete first** (reference implementation); EU VAT machinery next; UK and US as community packs behind the same interface. Supersedes the wave-1 choice of all four in v1 | Wave 1 → Wave 3 (Q-02) | Locked |
| D-005 | LLM engine: **the user's own Claude Code CLI + sky-la MCP tools**, under the compliance constraints in `REVIEW.md` §4.1. MCP server is a thin shim to the core. Launch profile uses `--restricted`, not `--bare`. A **provider trait** adds an **Anthropic API-key driver** beside the CLI | Wave 1 → Wave 3 (Q-03) | Locked |
| D-006 | Security: **SQLCipher + OS keychain**; **audited consent gate on every LLM egress, MCP tool results included**; **local-only, no telemetry**; **recovery key + encrypted backups mandatory**; **opt-in, off-by-default public reference data** (ČNB FX rates, minisign-signed rule-pack updates), manual import always available | Wave 1 → Wave 3 (Q-06) | Locked |
| D-007 | **Hash-chained journal, on by default.** Verified on open and on export. Supersedes the wave-1 decline | Wave 1 → Wave 3 (Q-07) | Locked |
| D-008 | Ledger kernel: **double-entry core, single-entry (*daňová evidence*) as a projected view**; postings carry typed tax dimensions | Wave 2 | Locked |
| D-009 | Invoicing v1: **CZ issue + PDF + QR Platba + ISDOC**; **EN 16931 (UBL + CII)**; **recurring + dunning**. Peppol *transmission* and Factur-X hybrid PDF deferred | Wave 2 + review | Locked |
| D-010 | Build order: scaffold → **CLI spike (timeboxed)** → money + store → kernel → shell | Wave 2 + review | Locked |
| D-011 | Design: **direction A "Tahoe" sets the home screen and the visual language** (floating glass source list, list + inspector, toolbar capsules, system type). Other screens may borrow patterns from B, C and D inside A's chrome | Wave 3 (Q-01) | Locked |
| D-012 | Tax arithmetic lives in Rust; the LLM never computes figures shown to the user (numeric grounding) | Review | Locked |
| D-013 | Advisors produce *scenario analysis and drafts*, not tax advice | Review | Locked |
| D-014 | Development model: work packets sized to one session; headless-verifiable UI; recorded LLM fixtures | Review | Locked |
| D-015 | Advisor access in v1: **proposal inbox** + **inline "explain this"** on any figure, account or line. Written period review in v1.x; scoped chat after v1 | Wave 3 (Q-04) | Locked |
| D-016 | Licence: **AGPL-3.0-or-later for the application**; **Apache-2.0 for the engine crates** `skyla-money`, `skyla-ledger`, `skyla-rules` and the rule-pack format. Apache-2.0 crates must never depend on AGPL crates | Wave 3 (Q-05) | Locked |

## Open questions

| ID | Question | Options | Recommendation |
|---|---|---|---|
| Q-08 | What the next session builds | WP-00 + WP-01 · extend direction A to the full screen set first · both | Both: WP-00 + WP-01 in code; A screen set on the canvas in parallel |
| Q-09 | Direction A refinements | Dark appearance first-class from WP-08 · light-only in v1 · compact density mode | Dark first-class: re-stepped greys and elevation, not an inversion |

## Wave history

- **Wave 1:** personas, jurisdictions, LLM wiring, security guarantees.
- **Wave 2:** design round 1 (rejected: "outdated typography, section design and UI quality"), ledger kernel, invoicing scope, first slices. A sixth, macOS-native round-1 direction was added and then superseded.
- **Review (round 2):** full solution review (`REVIEW.md`), with four new information-architecture directions A–D.
- **Wave 3:** A locked; CZ-first, API driver, hash chain on and opt-in reference data accepted; inbox + explain-this; AGPL app + Apache engine.
