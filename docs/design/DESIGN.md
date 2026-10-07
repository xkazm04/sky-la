# sky-la — Design Document

| | |
|---|---|
| **Status** | v2.1: architecture and design direction locked (see [`DECISIONS.md`](./DECISIONS.md)) |
| **Date** | 2026-10-07 |
| **Read with** | [`REVIEW.md`](./REVIEW.md) (why things are the way they are) · [`../plan/IMPLEMENTATION_PLAN.md`](../plan/IMPLEMENTATION_PLAN.md) (how we build it) |
| **Design canvas** | sky-la Design Directions, a private claude.ai artifact. **Direction A (Tahoe) is locked** |

---

## 1. Product

### 1.1 One line

**sky-la is open-source, local-first, encrypted desktop accounting for Czech freelancers and micro-companies. Its AI advisors propose, explain and calculate, but never post.**

### 1.2 Personas

| | P1: Freelancer (OSVČ) | P2: Micro company (s.r.o.) |
|---|---|---|
| Legal record | *Daňová evidence* (single-entry, cash basis) | *Účetnictví* (double-entry, accrual) |
| VAT | Payer or non-payer; monthly or quarterly | Usually a monthly payer |
| Annual tax | DPFO with §7 income; flat-rate vs actual expenses; possibly *paušální daň* | DPPO, prepared with or by an accountant |
| Insurance | Social and health advances and annual overviews | Employer side handled by external payroll |
| v1 boundary | Full | **Payroll is imported** as journals from an external payroll provider or accountant. sky-la doesn't calculate payroll in v1 |

### 1.3 Jobs to be done, in priority order

1. **Bill and get paid.** Issue a correct invoice in under a minute, with automatic reminders.
2. **Know what I owe, and when.** VAT, kontrolní hlášení, insurance advances and the annual return, on one obligations calendar.
3. **Reconcile the bank in minutes.** Import, auto-match, then decide only the residue.
4. **Close the period with confidence.** Checks pass, the statements are locked, and the books balance by construction.
5. **Produce the filings.** EPO-ready XML and an accountant package.
6. **Understand and improve.** Advisors that find money and explain every number.

### 1.4 Product principles

| # | Principle | Concretely |
|---|---|---|
| P1 | **Correct by construction** | Invariants are enforced in SQL *and* Rust. Money is integer minor units. Posted entries are immutable |
| P2 | **Nothing posts without you** | Rules, the matcher and advisors create *proposals*. Posting is a human act, or an explicit rule the human created |
| P3 | **Every number explains itself** | Every figure drills to its journal entries. Every advisor number traces to an engine computation |
| P4 | **Deterministic first, AI for the residue** | Rules and scoring clear most of the work. The LLM sees only what code couldn't decide, with the minimum data |
| P5 | **Private by default, auditable always** | No telemetry. Every byte sent to an LLM is consented to, minimised, logged and replayable |
| P6 | **Desktop-grade** | Keyboard-first, under 100 ms for local interactions, dense layouts, native-feeling chrome |
| P7 | **Plain language, jargon one click away** | P1 users see categories. Account numbers are visible but secondary |

### 1.5 Position

| Product | Model | Gap sky-la fills |
|---|---|---|
| Pohoda, Money S3 | Desktop, dominant in CZ | Dated UX, no AI, closed |
| ABRA Flexi, iDoklad, Fakturoid | Cloud SaaS | Data leaves your control; invoicing-centric (Fakturoid, iDoklad) |
| Xero, QuickBooks, FreeAgent | Cloud, not CZ-native | No Czech tax, cloud only |
| GnuCash, Beancount, Akaunting, ERPNext | Open source | No Czech tax pack, expert UX or web-first |

**sky-la's slot:** open source, local-first and encrypted, Czech-native tax, AI with auditable egress, and modern desktop UX.

### 1.6 v1 success metrics

| Metric | Target |
|---|---|
| Install → first issued invoice | < 3 min |
| Bank lines auto-matched by rules after one month of use | ≥ 85 % |
| Monthly close for a P1 user | < 15 min |
| Ledger invariant violations (property tests + fuzzing, every CI run) | 0 |
| Advisor figures reproducible from engine values | 100 % |
| Cold start | < 1.5 s |
| Scrolling a 100 000-line journal | 60 fps |

### 1.7 v1 non-goals

Payroll calculation · multi-entity and consolidation · inventory · cloud sync · direct bank APIs (import only) · direct submission to EPO or datová schránka (we generate the XML; the user uploads it) · Peppol transmission (we generate UBL; sending needs a certified Access Point) · inbound bill capture via LLM · Factur-X hybrid PDF.

---

## 2. v1 module scope

| Module | v1 contents |
|---|---|
| **Ledger kernel** | Double-entry journal; Czech chart of accounts (*směrná účtová osnova*) with a category layer for P1; periods and locks; reversals; projections: trial balance, P&L, balance sheet, cash basis (*daňová evidence*), VAT ledger |
| **Invoicing** | Issue (*daňový doklad*); credit note (*opravný daňový doklad*); proforma and advance (*zálohová faktura* + *daňový doklad k přijaté platbě*); number series with gap detection; PDF (Typst) with **QR Platba (SPAYD)**; **ISDOC 6** XML; **EN 16931**: UBL 2.1 (Peppol BIS Billing 3.0) + CII; recurring templates; dunning sequences with statutory late interest |
| **Bank** | Import **CAMT.053**, **MT940**, **ABO/GPC**, CSV with saved column profiles; deduplication; **statement tie-out** (opening + movements = reported closing); rules; explainable scoring matcher (amount, VS/KS/SS symbols, IBAN, name, date window); splits; reconciliation workbench |
| **Tax CZ** (`cz-2026` pack) | DPH return, kontrolní hlášení, souhrnné hlášení computation + EPO XML; DPFO §7 worksheet + flat-rate vs actual comparison; *paušální daň* eligibility check; insurance overviews data; obligations calendar |
| **Advisors** | Tax scenario advisor; financial advisor (variance and inefficiency detectors + explanation). Users reach them through the **proposal inbox** and **inline "explain this"** on any figure, account or line (D-015). Written period review in v1.x; scoped chat after v1 |
| **Security** | SQLCipher at rest; OS keychain; recovery key; encrypted scheduled backups; egress gate and register; auto-lock |
| **Portability** | Full export (journal JSON + CSV, document archive with ISDOC/PDF); import from Pohoda and Fakturoid exports (v1.x) |

---

## 3. Architecture

### 3.1 Process and trust boundaries

```
┌──────────────────────────────── sky-la (Tauri v2) ────────────────────────────────┐
│                                                                                    │
│  ┌─────────────── WebView ───────────────┐      ┌────────── Rust core ───────────┐ │
│  │ React 19 + TS · design system         │ IPC  │ commands (typed, tauri-specta) │ │
│  │ renders untrusted imported text       │─────▶│ ├ ledger kernel                │ │
│  │ holds NO secrets, enforces NO rules   │      │ ├ invoicing · bank · tax-cz    │ │
│  │ CSP: no remote, connect-src none      │      │ ├ advisor orchestrator         │ │
│  └───────────────────────────────────────┘      │ ├ egress gate + register  ◀─┐  │ │
│                                                 │ └ store: SQLCipher, 1 writer│  │ │
│                                                 └──────────────▲──────────────┼──┘ │
│                                                   local socket │ token-auth   │    │
│  ┌──────── user-installed `claude` (unmodified) ────────┐      │              │    │
│  │ -p --restricted --tools "" --strict-mcp-config …     │stdio┌┴─────────────┐│    │
│  │ talks to Anthropic with the USER's own credentials   │────▶│ skyla-mcp    ││    │
│  └──────────────────────────────────────────────────────┘     │ (thin shim)  ├┘    │
│                                                               └──────────────┘     │
└────────────────────────────────────────────────────────────────────────────────────┘
```

| Zone | Trust | Rule |
|---|---|---|
| **T1 WebView** | Low | Renders imported text, so treat it as hostile-adjacent. Holds no key and no invariants. Every command is re-validated in Rust |
| **T2 Rust core** | Trusted | The only holder of the database key and the only writer. Owns every invariant and the egress gate |
| **T3 `claude` subprocess** | Untrusted output | Its output is parsed against a schema, validated and never executed. Its network egress is to Anthropic under the user's own account |
| **T4 Imported files** | Untrusted input | Bank statements, CSV and XML. All parsers are fuzzed |

### 3.2 Repository layout

```
crates/
  skyla-money      Money(i64 minor, Currency), Rate(Decimal), rounding modes, allocation
  skyla-ledger     accounts, entries, postings + dimensions, invariants, periods, projections
  skyla-store      rusqlite + bundled SQLCipher, migrations, single-writer actor, triggers, backup
  skyla-rules      rule-pack loader: versioned TOML data, effective dating, citations
  skyla-tax-cz     CZ computations, scenario engine levers, EPO XML writers
  skyla-invoicing  documents, series, lifecycle, ISDOC / UBL / CII writers, SPAYD
  skyla-render     Typst templates → PDF
  skyla-bank       parsers (camt053, mt940, gpc, csv), normaliser, dedupe, tie-out, matcher, rules
  skyla-egress     scopes, policies, pseudonymisation, register
  skyla-advisor    LlmProvider trait; ClaudeCodeCli, AnthropicApi and Fake drivers; prompts; schemas
  skyla-mcp        stdio MCP server shim → core over authenticated local socket
apps/
  desktop          Tauri shell (src-tauri) + React app (src)
packages/
  ui               design system: tokens, primitives, components
  ipc              generated bindings + transports (tauri | mock with fixtures)
  fixtures         shared demo dataset (the entity used in the design canvas)
rules/
  cz/2026/         pack.toml + citations.md
docs/
```

### 3.3 Ledger kernel

**Core tables (simplified):**

```text
account(id, code, name, kind[asset|liability|equity|revenue|expense|off_balance],
        normal_side, parent_id, is_leaf, active, category_id, tax_map)
period(id, starts_on, ends_on, state[open|closing|closed])
journal_entry(id ULID, period_id, date, status[draft|posted|reversed], source_kind[manual|invoice|bank|rule|advisor],
              source_id, memo, created_by, approved_by, posted_at, reverses_id, chain_hash?)
posting(entry_id, line_no, account_id, amount_minor i64 (debit +, credit −), currency,
        amount_func_minor i64, fx_rate_id, vat_code, tax_treatment[deductible|non_deductible|exempt|n/a],
        settles_document_id, counterparty_id, dims JSON)
```

**Invariants**, each enforced twice: in SQL triggers (so even raw SQL can't violate them) and in Rust (for good error messages).

| ID | Invariant |
|---|---|
| I1 | A posted entry balances: Σ `amount_func_minor` = 0 |
| I2 | Posted entries and postings are immutable (`UPDATE`/`DELETE` → `ABORT`). Corrections are reversal entries |
| I3 | No posting dated into a `closed` period |
| I4 | Postings reference active **leaf** accounts only |
| I5 | Document number series are unique, with gaps detected and reported (required for *daňové doklady*) |
| I6 | No floating point in money paths (clippy `float_arithmetic` denied in money and ledger crates) |
| I7 | Every advisor- or rule-created entry carries `approved_by` before it can be posted |

**Projections** are pure functions of the journal: trial balance, P&L, balance sheet, VAT ledger by form row, and **cash basis (*daňová evidence*)**. The cash basis recognises income and expense at *settlement*, using the `tax_treatment` and `settles_document_id` dimensions. Every report stamps an **input snapshot hash** (hash of the posted entries it read), so any statement can be reproduced exactly.

**Tamper evidence (on by default, D-007):** `chain_hash = H(prev_chain_hash ‖ canonical(entry))` is computed in the posting transaction. A verifier runs on open and on export, and a break is a hard error with the first bad entry named.

**FX:** functional currency CZK. Rates come from the ČNB daily fixing via the opt-in reference-data fetch (§3.9) or manual import. The rate source is stored on every posting.

### 3.4 Rule packs

- **Data** lives in `rules/<country>/<year>/pack.toml`, with every value effective-dated and cited: VAT rates (CZ: 21 % and 12 %), fixed-asset threshold (80 000 Kč), flat-rate percentages and caps (60 % → max 1 200 000 Kč, …), insurance parameters, deadlines (e.g. DPH and KH on the 25th), form-row mappings, late-interest formula (ČNB repo rate + 8 p.p.).
- **Computation** lives in Rust per jurisdiction (`skyla-tax-cz`), golden-tested against official worked examples and hand-verified cases.
- **Provenance:** every output records `pack_id@version`, so a return generated today can be regenerated identically next year.
- **No DSL.** Community packs (UK, US) implement the same Rust traits. Sandboxed WASM packs are a v2 consideration.

### 3.5 Invoicing

Lifecycle: `Draft → Issued (number assigned; immutable; posts AR + VAT) → Sent → Partially paid → Paid | Credited | Written off`. Corrections are always credit notes.

- **Outputs:** PDF via Typst with a SPAYD QR code · ISDOC 6 · UBL 2.1 (Peppol BIS Billing 3.0) · CII. Validated against XSD, and against the EN 16931 Schematron in CI (KoSIT validator container).
- **Recurring:** template + schedule → drafts, or auto-issue if the user opts in.
- **Dunning:** three-step sequence with tone templates. Statutory late interest is computed from the pack.
- **Sending:** via the system mail client with attachments. No network from the app in v1. SMTP is a later opt-in.

### 3.6 Bank import and reconciliation

```
parse → normalise(BankLine) → dedupe(hash: account, date, amount, ref, sequence) → tie-out(opening + Σ = closing)
      → user rules → scorer (signals with weights, explained) → auto-accept ≥ threshold (opt-in per rule)
      → residue → advisor (minimum fields) → human
```

- **Signals:** exact or near amount, VS = invoice number, KS/SS, known payer IBAN, fuzzy name, date window relative to due date. Every score is shown as a sum of named contributions (direction D).
- **Learning:** accepting a match can create a rule ("auto-accept Northwind when amount and VS match"). Rules are visible and editable.
- **Formats:** CAMT.053, MT940, ABO/GPC (common Czech bank export), CSV with a saved per-bank column profile. Every parser has a `cargo-fuzz` target.

### 3.7 Advisors: the LLM pillar

#### 3.7.1 Provider abstraction

```rust
trait LlmProvider {
    fn capabilities(&self) -> ProviderCaps;                 // structured output, tools, streaming, cost reporting
    fn run(&self, task: AdvisorTask) -> BoxStream<AdvisorEvent>;
}
// drivers: ClaudeCodeCli (default v1) · AnthropicApi (API key in OS keychain) · Fake (recorded fixtures)
```

- **`ClaudeCodeCli`** runs the user-installed, unmodified `claude` binary with the launch profile in [`REVIEW.md` §4.2](./REVIEW.md). It reads `system/init`, `assistant`, `user` (tool results), `result` (`structured_output`, `total_cost_usd`) and `system/api_retry` events from stream-json, and aborts the run if `init` shows any tool, MCP server, command or skill beyond the hardened set (`skyla_advisor::cli_stream::profile_violations`). Findings: [`docs/spikes/WP-01-cli.md`](../spikes/WP-01-cli.md).
- **`AnthropicApi`** speaks raw HTTPS (there's no official Rust SDK) to the Messages API. Model defaults to `claude-opus-5-5` with explicit `output_config.effort`; `claude-sonnet-5-5` or `claude-haiku-4-5` for high-volume ranking. Uses structured outputs (`output_config.format`), not forced `tool_choice`, which current models reject. Enables server-side refusal fallbacks as the API docs recommend.
- **`Fake`** replays recorded transcripts. All CI and all cloud sessions use it unless a live eval is explicitly invoked.

#### 3.7.2 Compliance requirements for the CLI driver

From the Claude Code legal and compliance docs:

- Detect the user-installed binary. Never bundle or modify it.
- Never read, store or proxy Claude credentials. Sign-in happens in the user's terminal through `claude auth login`.
- Never pay for, resell or intermediate usage.
- Keep usage event-driven and individual-scale.
- No Claude, Claude Code or Anthropic names or logos in product or feature names.

#### 3.7.3 MCP tool surface: read and propose only

| Tool | Kind | Notes |
|---|---|---|
| `get_period_summary(period)` | read | aggregates only |
| `get_account_balances(filter)` | read | account-level totals |
| `get_entries(filter, limit)` | read | redacted per granted scope; counterparties pseudonymised unless granted |
| `get_vat_position(period)` | read | by form row |
| `get_rule_pack_value(key)` | read | with citation |
| `run_scenario(spec)` | compute | **the deterministic engine computes**; the model can't do arithmetic itself |
| `propose_entry(draft)` | propose | kernel-validated (balance, period, accounts), stored as a proposal |
| `propose_categorisation(line, account, reason)` | propose | for the residue lines |
| `propose_finding(finding)` | propose | must reference engine values by ID |
| `ask_user(question)` | propose | becomes an inbox item: facts the books can't contain |

There are no write, post, delete, network or filesystem tools.

#### 3.7.4 Tax advisor = scenario engine + explainer

1. The pack declares **levers**: flat-rate vs actual expenses; *paušální daň* eligibility; asset purchase timing vs the 80 000 Kč threshold; VAT registration threshold watch; insurance advance adjustment. For P2 in v2: salary vs dividend.
2. The engine enumerates the applicable lever combinations and **computes** tax, insurance and side effects. Side effects include the lower pension assessment base that comes with lower insurance.
3. The model selects plausible levers given the facts, asks `ask_user` questions for facts the ledger can't know, ranks the options and writes the explanation.
4. A **numeric-grounding validator** checks every number in the model's prose against engine outputs. Any mismatch rejects the run.
5. The output is a *scenario analysis* artifact with workings, assumptions and a "for your or your advisor's review" frame. It's exportable in the accountant package.

#### 3.7.5 Financial advisor = detectors + explainer

Deterministic detectors produce candidate findings:

- Variance by account or vendor vs the prior period.
- Vendor rate changes, e.g. hourly rate derived from bill lines.
- Margin by client.
- Late-payer profiles.
- Subscription creep.
- Cash runway.
- Concentration risk.

The model de-duplicates, ranks, explains and proposes actions. Every finding cites its source entries.

#### 3.7.6 Egress gate and register

- Every advisor task declares a **scope**: a set of field classes (`aggregates`, `account_totals`, `counterparty_names`, `line_memos`, `documents`). The user sets a policy per task type: *always*, *ask* or *never*.
- **Redaction:** IBANs and personal IDs are never sent. Counterparty names are replaced by stable pseudonyms ("Vendor A") unless the scope grants names.
- **Tool results pass through the gate.** They are egress too.
- **Register:** an append-only, locally encrypted record per run: time, task, driver, model, scopes, the exact payloads (for replay), reported cost and tokens. It's visible in the UI as "What was shared".

### 3.8 Security model

| Area | Design |
|---|---|
| **At rest** | SQLCipher 4. A random 256-bit **DEK** is wrapped by (a) a KEK derived from the passphrase (Argon2id), cached in the OS keychain for convenience with Touch ID / Windows Hello where available, and (b) a **recovery key** printed at setup |
| **Backups** | Scheduled encrypted snapshots (SQLCipher backup API) to a user-chosen folder. Restore runs in CI |
| **Process** | Tauri v2 capabilities: minimum, with no fs, shell or http plugins exposed to the webview. Strict CSP. Isolation pattern. Typed command inputs re-validated in Rust. Secrets zeroised |
| **Network** | No telemetry. Only three paths: (1) the user's `claude` process; (2) **opt-in, off by default** public reference data (ČNB FX rates; rule-pack updates signed with minisign), with manual import always available (D-006); (3) opt-in update check. Only the reference-data module links an HTTP client |
| **Supply chain** | `cargo-deny`, `cargo-audit`, a pinned lockfile, SBOM, signed and notarised releases, reproducible-build goal |
| **Session** | Auto-lock on idle; lock on sleep; re-auth for exports and key changes |

**Threat model summary:**

| Threat | Mitigation |
|---|---|
| Stolen laptop | SQLCipher, keychain-gated KEK, auto-lock |
| Malicious bank file | Fuzzed parsers; parsing outside the webview |
| Prompt injection via memos | Propose-only tools; kernel validation; human approval; fencing |
| Exfiltration via LLM | Scoped gate on prompts *and* tool results; pseudonymisation; register |
| Compromised webview | Holds no key; core re-validates; no fs or shell |
| Silent corruption | Invariant triggers; projection checksums; optional hash chain |
| Lost passphrase | Recovery key; backups |

### 3.9 Frontend

- **Stack:** React 19 · TypeScript strict · Vite · Tailwind v4 (tokens as CSS variables; light and dark) · React Aria Components · TanStack Router / Query / Table / Virtual · Zustand for ephemeral UI state only.
- **Headless-verifiable:** `packages/ipc` ships a **mock transport** backed by `packages/fixtures`, so the whole UI runs in a plain browser. Playwright on Chromium then provides screenshots and e2e in cloud sessions without a display.
- **Internationalisation:** Czech and English from day one (ICU messages); `Intl` formatting with `cs-CZ` as default (`84 700,00 Kč`).
- **Platform-adaptive chrome:** macOS vibrancy, traffic lights and Tahoe-style floating sidebar; Windows title-bar controls; Linux plain. Backdrop blur is feature-detected.
- **Performance budgets:** route transition < 100 ms on fixtures; tables virtualised beyond 200 rows.

### 3.10 Testing strategy

| Layer | Tooling | Gate |
|---|---|---|
| Kernel invariants | `proptest` (random balanced and unbalanced entries; raw-SQL tampering attempts) | every CI run |
| Money | unit + property tests (allocation sums, rounding modes) | every CI run |
| Outputs (XML, PDF) | `insta` snapshots + XSD validation; EN 16931 Schematron via KoSIT container | every CI run |
| Parsers | `cargo-fuzz` corpus replay in CI; long fuzz runs on demand | every CI run (replay) |
| UI | Vitest; Playwright against the mock-IPC build, with screenshots | every CI run |
| Advisors | `Fake` transcripts always; live eval set (≥ 30 CZ cases) on demand | on demand |

---

## 4. Jurisdiction strategy

1. **CZ is the reference pack**, complete, cited and golden-tested.
2. **EU VAT machinery** comes out of CZ work: reverse charge, intra-EU supply, EC sales list, EN 16931. OSS follows.
3. **UK and US are community packs** behind the same traits, with a contributor kit (template pack, golden-test harness, citation requirements).

---

## 5. Legal frame

- **Advisors are not tax advice.** Czech tax advisory is a regulated profession. UI copy uses "scenario", "draft" and "for your review". Every advisor artifact can go into the **accountant package**.
- **Privacy:** processing is local. LLM egress happens only under the user's own Anthropic account and terms, and every transfer is in the register.
- **Retention:** statutory multi-year retention makes backup, recovery and export correctness features.
- **Trademark:** the product may say it runs or uses Claude Code. It must not use Claude or Anthropic names or logos in product or feature names.
- **Licence (D-016):** the application is AGPL-3.0-or-later. The engine crates `skyla-money`, `skyla-ledger` and `skyla-rules`, and the rule-pack format, are Apache-2.0 so others can build on the correctness work. Apache-2.0 crates never depend on AGPL crates.

---

## 6. Design language: direction A, "Tahoe" (locked, D-011)

A macOS 26-era native three-pane language, applied on every platform with platform-adaptive window chrome.

| Element | Specification |
|---|---|
| **Window** | A window backdrop with three panes. Side panes are **floating glass panels** inset 8 px from the window edge. Radii are concentric with the window (panels 18 px, inner groups 12–14 px, controls fully rounded) |
| **Source list** (left) | 28 px rows, section labels at 11 px semibold secondary. The selected row gets an accent-tinted capsule with an accent icon, not a full accent fill. Counts and badges trail the row. The entity switcher sits at the bottom |
| **Toolbar** | No solid bar. The title and a subtitle (key figures) sit at the content's top-left. Controls are **glass capsules**: a segmented filter, search, an icon group, and one accent primary action |
| **Content** | One white list or table group per view: 40 px rows, hairline separators, section headers in-list ("Needs attention", "Drafts and scheduled", "Paid"), status pills with icon + label (never colour alone) |
| **Inspector** (right) | The selected object: a document preview (e.g. the invoice paper with QR Platba), activity timeline, inline advisor note ("explain this" / finding), actions pinned to the bottom |
| **Status line** | 30 px. Encryption state, journal integrity (balanced, hash chain), egress summary ("Nothing sent to Claude today") |
| **Type** | System stack (`-apple-system` → SF Pro; Geist fallback off-Mac); 11 / 11.5 / 12.5 / 13 / 15 / 17 px; tracking tightens with size; **tabular figures in the sans**, with no decorative monospace |
| **Colour** | Neutral greys, one accent (system blue by default, user-tweakable), and semantic status colours with matched-lightness pill backgrounds. Glass is `backdrop-filter` blur + saturation, with an opaque fallback where blur is unavailable |

**Borrowed patterns inside A's chrome:**
- **Inbox** (from B): a source-list item whose list holds decisions and whose inspector shows the proposal (journal entry, reasons, approve).
- **Statements** (from C): a document-style content pane with drill-down rows, an accrual vs cash toggle, and advisor notes in the inspector.
- **Reconciliation** (from D): bank lines as the list, candidates with an explained score breakdown, and the posting and its effects in the inspector, with statement tie-out in the subtitle.

**Dark appearance is first-class from WP-08** (D-017): re-stepped greys and elevation, not an inversion. Every colour is a token with a light and a dark value, and every screen's Playwright check runs in both appearances.
