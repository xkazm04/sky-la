# sky-la features: module documentation

What each part of sky-la does today, where it lives, and what it doesn't do yet. These pages describe the **current state** (October 2026, after WP-00 – WP-34 and improvement waves 1–15). For the reasons behind the design read `docs/design/`; for the history and the open items, `docs/plan/STATUS.md`; for end-user instructions, `docs/user/`.

| Page | Covers | Main code |
|---|---|---|
| [Ledger kernel](ledger.md) | Money, accounts, posting, invariants, periods, reversals, the hash chain, reports | `skyla-money`, `skyla-ledger` |
| [Encrypted storage and sessions](storage-and-security.md) | SQLCipher, the vault and recovery key, backups, unlock, auto-lock, the security review | `skyla-store`, `skyla-app::session`, the shell's `Session` |
| [Rule packs](rule-packs.md) | Statutory data with citations, the calendar, golden cases, signed updates, reference data | `skyla-rules`, `rules/` |
| [Invoicing](invoicing.md) | Drafts, issuing, credit notes and advances, PDFs, ISDOC/UBL/CII, recurring invoices, reminders | `skyla-invoicing`, `skyla-render` |
| [Purchases](purchases.md) | Received invoices, payables, VAT deduction | `skyla-app::core::purchases` |
| [Bank](bank.md) | Statement parsers, tie-out, rules, the matcher, the reconciliation workbench | `skyla-bank`, `skyla-app::core::bank` |
| [Taxes](taxes.md) | DPH return, kontrolní hlášení, § 7 worksheet, scenarios, obligations calendar | `skyla-tax-cz`, `skyla-app::core::tax` |
| [Inbox](inbox.md) | Proposals, approvals, deadlines, advice, dismissals | `skyla-app::core::inbox` |
| [Advisors](advisors.md) | The Claude Code CLI driver, MCP tools, the egress gate and register, grounding, findings, "explain this" | `skyla-advisor`, `skyla-mcp`, `skyla-egress` |
| [Import and export](import-export.md) | Pohoda and Fakturoid import, the full export | `skyla-invoicing::import`, `skyla-app::core::{imports, export}` |
| [Desktop app and UI](desktop-app.md) | The Tauri shell, typed IPC, the mock transport, the design system, screens, e2e | `apps/desktop`, `packages/{ipc,ui,fixtures}` |
| [Release and updates](release.md) | Packaging, signing, the opt-in update check | `skyla-release`, `.github/workflows/release.yml` |

## Architecture at a glance

```
 webview (React 19, no secrets, no rules)
   │  generated typed commands (packages/ipc/src/bindings.ts)
   ▼
 Tauri shell (apps/desktop/src-tauri): Session holds the open books; every command takes `Books`
   │
   ▼
 skyla-app::Core: application services, DTOs, validation, persistence of app state
   ├── skyla-ledger ── skyla-money          (Apache-2.0 kernel: posting, invariants, reports)
   ├── skyla-rules                          (Apache-2.0: rule packs, calendar, golden cases)
   ├── skyla-invoicing ── skyla-render      (documents, exchange formats, PDFs)
   ├── skyla-bank                           (parsers, matcher)
   ├── skyla-tax-cz                         (KH, § 7 worksheet, scenarios)
   ├── skyla-egress                         (gate, pseudonyms, register)
   ├── skyla-advisor ⇄ claude (user's own CLI) ⇄ skyla-mcp shim ⇄ core's loopback tool host
   └── skyla-store                          (SQLCipher, vault, backups, keychain)
```

## The rules every module keeps

- **Money is integer minor units.** Floats are denied by clippy in money and ledger paths.
- **Posted entries never change.** A correction is a reversal entry; triggers enforce it as well as Rust.
- **Every posted entry balances**, lands in an open period and targets active leaf accounts.
- **Nothing an advisor or a rule produces is posted** until a person approves it, and the kernel re-validates it then.
- **Statutory values come from the rule pack** with a citation; logic asks for a value on a date.
- **The webview enforces nothing.** The core checks every command again.
- **No network** except the user's own `claude` process, opt-in signed reference data and an opt-in update check. No telemetry.

## Numbers (at the time of writing)

- 13 engine crates and the Tauri shell; 74 typed IPC commands.
- 305 Rust tests (unit, integration and property tests), plus fuzz targets for the bank parsers.
- 63 Playwright runs: every screen in light and dark with axe (WCAG 2.2 AA) and pixel baselines, and the end-to-end flows.
- 60 hand-worked golden cases for the CZ 2026 rule pack.
