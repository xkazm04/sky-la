# sky-la — Decision Log

Each decision records what was asked, what was chosen, and its current status after review.
Status values: **Locked** · **Revised** (changed by review, pending confirmation) · **Open**.

## Locked and revised decisions

| ID | Decision | Source | Status |
|---|---|---|---|
| D-001 | Brand name **sky-la** | Brief | Locked |
| D-002 | Desktop app on **Tauri**; **open source**; **React + TypeScript + Tailwind** client | Brief | Locked |
| D-003 | v1 personas: **freelancer (OSVČ)** and **small business**, one entity each | Wave 1 | Locked |
| D-004 | Jurisdictions: user chose **CZ, EU, US and UK** | Wave 1 | **Revised.** Review proposes the CZ pack complete first, EU VAT next, UK and US as community packs behind the same interface (Q-02) |
| D-005 | LLM engine: **Claude Code CLI sidecar + MCP ledger tools** | Wave 1 | **Revised.** Kept, under the compliance constraints in `REVIEW.md` §4.1. The MCP server becomes a thin shim to the core. The launch profile uses `--restricted`, not `--bare`. A provider trait adds an API-key driver (Q-03) |
| D-006 | Security: **SQLCipher + OS keychain**; **audited consent gate on every LLM egress**; **local-only, no telemetry** | Wave 1 | Locked, **extended**: recovery key and encrypted backups are mandatory; the gate covers MCP tool results; opt-in public reference data (Q-06) |
| D-007 | Hash-chained journal | Wave 1 (declined) | **Open again** (Q-07). Cheap and high-value; review recommends default-on |
| D-008 | Ledger kernel: **double-entry core, single-entry (*daňová evidence*) as a projected view** | Wave 2 | Locked; postings gain typed tax dimensions |
| D-009 | Invoicing v1: **CZ issue + PDF + QR Platba + ISDOC**; **EN 16931 / Peppol BIS**; **recurring + dunning** | Wave 2 | Locked, **trimmed**: Peppol *transmission* and Factur-X hybrid PDF deferred; generation stays |
| D-010 | First build slices: **ledger kernel**, **MCP + CLI spike**, **Tauri shell + design system on fixtures** | Wave 2 | **Revised order**: scaffold → spike (timeboxed) → money + store → kernel → shell |
| D-011 | Design round 1 (six dashboard skins) | Wave 2 / 3 | **Rejected by user.** Round 2 has four information-architecture theses (A–D) |
| D-012 | Tax arithmetic lives in Rust; the LLM never computes figures shown to the user | Review | Locked (principle P3, numeric grounding) |
| D-013 | Advisors produce *scenario analysis and drafts*, not tax advice | Review | Locked (legal frame) |
| D-014 | Development model: work packets sized to one session, headless-verifiable UI, recorded LLM fixtures | Review | Locked |

## Open questions (wave 3)

| ID | Question | Options | Recommendation |
|---|---|---|---|
| Q-01 | Design direction | A Tahoe · B Inbox · C Statement · D Workbench · a synthesis | A's chrome + B as home + C statements + D reconciliation |
| Q-02 | Accept the jurisdiction re-scope? | CZ-first, the rest via the interface · keep all four in v1 | CZ-first |
| Q-03 | Add an API-key driver beside the CLI? | Yes (provider trait) · CLI only | Yes. It covers users without a subscription and live evals, and hedges policy risk |
| Q-04 | How are advisors reached? | Proposal inbox · inline "explain this" · scoped chat · written periodic review | Inbox + explain-this in v1; written review in v1.x; chat last |
| Q-05 | Licence | AGPL-3.0 · Apache-2.0 · MIT · AGPL app + Apache engine crates | AGPL app + Apache engine crates |
| Q-06 | Network for public reference data (ČNB FX rates, signed rule-pack updates)? | Opt-in fetch · manual import only | Opt-in fetch, off by default, manual import always available |
| Q-07 | Hash-chained journal default? | On · opt-in · off | On |
| Q-08 | Next session's spend | Scaffold + spike · design system · kernel | Scaffold (WP-00) + spike (WP-01) |
