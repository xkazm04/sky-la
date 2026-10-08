# sky-la

**Accounting that stays on your machine.** Open-source, encrypted, desktop-native books for Czech freelancers (OSVČ) and micro-companies, with AI advisors that explain and suggest, but never touch your ledger without you.

> Status: **preview (0.1).** Every work packet of the v1 plan is built and tested; the first signed release waits on the maintainer's signing keys. The Czech rule pack is a *draft*: check every figure before you file. Not tax advice.

## What it does

- Double-entry books with integer money, immutable posted entries and a verified hash chain, in one SQLCipher-encrypted file
- Invoices and credit notes: Czech PDF with QR Platba, ISDOC, UBL and CII
- Bank statements (camt.053, MT940, ABO/GPC, CSV) with tie-out, deduplication and explained matches
- The DPH return, the kontrolní hlášení, the § 7 income-tax worksheet with insurance scenarios, and an obligations calendar
- Scheduled encrypted backups with a restore check; a full export in open formats; import from Pohoda and Fakturoid
- Tax and financial advisors through your own Claude Code, which only propose, cite and calculate with the engine's figures

## Documents

| For | Read |
|---|---|
| Users | [`docs/user/`](docs/user/README.md): getting started, everyday work, taxes, advisors, your data, troubleshooting |
| Rule-pack contributors | [`rules/README.md`](rules/README.md): the format, citation rules, golden cases |
| Developers | [`CLAUDE.md`](CLAUDE.md) (commands and invariants), [`docs/design/DESIGN.md`](docs/design/DESIGN.md) (architecture), [`docs/design/DECISIONS.md`](docs/design/DECISIONS.md) |
| Reviewers | [`docs/design/REVIEW.md`](docs/design/REVIEW.md) (the concept's review), [`docs/design/SECURITY_REVIEW.md`](docs/design/SECURITY_REVIEW.md) (threat model and audit) |
| Maintainers | [`docs/RELEASING.md`](docs/RELEASING.md), [`docs/plan/STATUS.md`](docs/plan/STATUS.md), [`docs/plan/IMPLEMENTATION_PLAN.md`](docs/plan/IMPLEMENTATION_PLAN.md) |

## Principles

1. Correct by construction: integer money, immutable posted entries, invariants in the database itself.
2. Nothing posts without you.
3. Every number explains itself.
4. Deterministic first; AI only for what code can't decide.
5. Private by default; every byte shared with an AI is logged and replayable.

## AI advisors

sky-la uses **your own Claude Code installation** for its tax and financial advisors. You sign in to Claude Code yourself, and sky-la never sees your credentials. Advisors have no network access of their own: they read through a narrow, audited interface and can only *propose*. Everything sent passes the egress gate, which withholds account and personal ID numbers and pseudonymises names, and is recorded in the register.

## Building from source

Rust (pinned in `rust-toolchain.toml`), Node 22 and pnpm 10; on Linux, WebKitGTK 4.1 for the desktop shell.

```
pnpm install
just dev-web       # the UI in a browser, on the recorded demo core (http://localhost:1420)
just ci            # the full gate: formatting, lints, licence and security audits, tests, e2e
pnpm --filter @skyla/desktop tauri dev   # the desktop app
```

## Licence

The application is **AGPL-3.0-or-later**. The engine crates (`skyla-money`, `skyla-ledger`, `skyla-rules`) and the rule-pack format are **Apache-2.0**, so the correctness work can be reused anywhere.
