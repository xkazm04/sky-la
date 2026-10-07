# sky-la — Solution Review

| | |
|---|---|
| **Status** | Review of record, round 2 |
| **Date** | 2026-10-07 |
| **Scope** | Product concept, technology stack, LLM integration, plan, and round 1 design prototypes |
| **Outcome** | Corrections are folded into [`DESIGN.md`](./DESIGN.md), [`DECISIONS.md`](./DECISIONS.md) and [`../plan/IMPLEMENTATION_PLAN.md`](../plan/IMPLEMENTATION_PLAN.md) |

This review treats every earlier decision as a hypothesis. Each verdict is **Keep**, **Change** or **Add**, with the reason. Claims that the architecture depends on were checked against primary sources (listed at the end), not memory.

---

## 1. Verdict summary

| # | Area | Before | Verdict | What changes |
|---|---|---|---|---|
| 1 | Positioning | "Professional accounting for anyone", four jurisdictions | **Change** | A sharp wedge: Czech freelancers (OSVČ) and micro s.r.o. first, EU VAT machinery second, UK and US through the same rule-pack interface as community packs |
| 2 | Jurisdictions | CZ, EU, US and UK all complete in v1 | **Change** | The CZ pack ships complete and is the reference implementation. The pack *interface* is multi-jurisdiction from day one. Four complete packs in v1 isn't achievable at the correctness bar this product needs |
| 3 | LLM engine | Claude Code CLI sidecar + MCP | **Keep, with hard constraints** | Permitted by Anthropic's published terms *only* as the unmodified, user-installed binary with the user's own sign-in (§4.1). A provider trait with an API-key driver is added as the second path |
| 4 | CLI launch profile | Implicitly "bare, scripted" | **Change** | `--bare` never reads subscription OAuth, so it can't serve subscription users. The hardened profile uses `--restricted --tools "" --strict-mcp-config` (§4.2) |
| 5 | MCP server | A server reading the ledger | **Change** | A thin stdio shim forwarding to the core over an authenticated local socket. The database key never leaves the core process, and the egress gate has a single choke point |
| 6 | Egress consent gate | Gate on outbound prompts | **Keep, extend** | MCP **tool results are egress too**. Add pseudonymisation and an encrypted, replayable payload register |
| 7 | Tax advisor | The LLM optimises taxes | **Change** | A deterministic **scenario engine** computes every number. The LLM picks levers, asks for missing facts and explains. A numeric-grounding validator rejects any figure in prose that the engine didn't produce |
| 8 | Ledger kernel | Double-entry core, single-entry projection | **Keep, specify** | Postings carry typed tax dimensions (VAT code, tax treatment, settlement link). Without them the *daňová evidence* projection is impossible |
| 9 | Hash-chained journal | Declined | **Reconsider** | About 50 lines of code for tamper evidence and silent-corruption detection. Recommended as default-on; still the user's call (open question Q-07) |
| 10 | Local-only, no network | Locked | **Keep, clarify** | Correct accounting needs ČNB daily FX rates and rule-pack updates. Add an opt-in, signed, *public-data-only* fetch, with manual import as the alternative |
| 11 | Encrypted at rest | SQLCipher + keychain | **Keep, add** | A **recovery key and encrypted backups are mandatory in v1.** Statutory retention runs to years. A forgotten passphrase must not destroy legally required records |
| 12 | Invoicing v1 | CZ + EN 16931 + recurring + dunning | **Keep, cut transmission** | Generate EN 16931 (UBL/CII). Peppol *transmission* requires a certified Access Point and is deferred. Factur-X hybrid PDFs are deferred (Typst PDF/A-3 attachment support isn't upstream). Sending uses the system mail client |
| 13 | Stack | Tauri + React + TS + Tailwind | **Keep, specify** | rusqlite with bundled SQLCipher, tauri-specta (pinned rc), React Aria Components, TanStack, Typst, proptest/insta/cargo-fuzz |
| 14 | Build order | Kernel → CLI spike → shell | **Adjust** | Scaffold → **CLI spike first** (timeboxed) → kernel → shell. The spike can change the architecture. The kernel is known territory |
| 15 | Development in cloud sessions | Implicit | **Add** | Headless-verifiable UI (mock IPC transport plus Playwright on the pre-installed Chromium), SessionStart hook, `STATUS.md`, and session-sized work packets with acceptance commands |
| 16 | Design | Six skins on one dashboard | **Change** | Discarded. Round 2 presents four *information-architecture theses* (§6) |
| 17 | Legal framing | Absent | **Add** | Tax advice is a regulated profession in Czechia (daňový poradce). Advisors produce scenario analysis and drafts for review, never advice. Claude trademark rules apply to naming |
| 18 | Adoption | Absent | **Add** | Import from Pohoda and Fakturoid, an "accountant package" export, and a deadlines and obligations calendar |

---

## 2. Product review

### 2.1 What is genuinely strong

- **The trust thesis is right and rare.** Every Czech competitor is either cloud SaaS (Fakturoid, iDoklad, ABRA Flexi) or a legacy desktop app with no AI and dated UX (Pohoda, Money S3). *Local-first + encrypted + AI that provably can't touch the books + every byte of egress on record* is a position nobody occupies. It is also the hardest to copy, because incumbents' architectures are built the other way.
- **"Bring your own Claude"** removes the inference bill from the project and the API-key burden from the user. It's permitted when done as specified in §4.1.
- **A double-entry kernel under a single-entry view** serves both personas with one provably correct engine. That's the right call, and the expensive one.

### 2.2 Where it fell short of excellent

1. **Too broad to be excellent at anything.** "Accounting for anyone, in four jurisdictions" spreads the correctness budget across every tax system at once, and correctness is the product. *Excellent* means being the best tool a Czech freelancer has ever used, then extending the same machinery.
2. **The real centre of the app was never named.** Users don't want a dashboard. They want to **decide** (approve a match, a posting, a filing) and to **trust a document** (an invoice, a return, a statement). Round 1 put a dashboard at the centre, which is why all six designs looked alike underneath.
3. **The LLM was placed as an oracle.** "Tax advisor optimises taxes" invites the model to do arithmetic and legal reasoning in prose. Both failure modes surfaced in our own round-1 mockups (§6.1). The fix is structural: an engine computes, the model chooses and explains.
4. **Missing jobs-to-be-done** that matter more to freelancers than any AI feature:
   - **An obligations calendar.** What's due when (DPH, kontrolní hlášení, insurance advances, the annual return), generated from the rule pack. This is the single most anxiety-reducing feature for the persona.
   - **Migration in.** Nobody switches accounting tools without importing history. Pohoda and Fakturoid imports are the adoption gate.
   - **The accountant hand-off.** Many OSVČ and most s.r.o. still use an accountant for the annual return. An "accountant package" export turns the accountant from a competitor into a distribution channel.
5. **The "payroll-adjacent" persona needs a boundary.** v1 can't calculate payroll (that's a separate regulated domain). It *must* import payroll journals from an external payroll provider or accountant. State this up front rather than let users discover it.
6. **No success metrics.** Excellence needs numbers. Proposed in `DESIGN.md` §1.6.

### 2.3 Legal and positioning risks

- **Regulated advice.** In Czechia, tax advisory is a licensed profession. Copy, flows and exports must frame the advisors as *scenario analysis and drafts for the user's or their advisor's review*. A "Package for my accountant" export is both the legal safety valve and a growth feature.
- **Trademark.** Per the Claude Code terms, the product may say in plain text that it "runs Claude Code". It may not use Claude, Claude Code or Anthropic names or logos in its own product or feature names, or imply endorsement. "sky-la" is fine. A feature called "Claude Advisor" would not be.
- **Retention.** Accounting and VAT records must be kept for years. That turns backup, recovery and export from conveniences into correctness requirements.

---

## 3. Technology stack review

| Component | Verdict | Notes |
|---|---|---|
| **Tauri v2** | Keep | A Rust core is where ledger correctness belongs. Small binaries and a capability-based permission model. Risk: WebKitGTK on Linux renders and performs worse than WKWebView or WebView2, so budget for Linux-specific QA |
| **React 19 + TypeScript (strict)** | Keep | Mature, and the agentic tooling knows it best. Using it doesn't need justifying |
| **Tailwind v4** | Keep | Tokens as CSS variables are needed for light and dark appearance. Write component classes once in the design-system package so markup doesn't repeat utility strings |
| **SQLite via SQLCipher** | Keep | Use `rusqlite` with `bundled-sqlcipher-vendored-openssl` (sync, reliable SQLCipher support), behind a single-writer actor. Prefer it to `sqlx`, whose SQLCipher story is weaker |
| **Typed IPC** | Add | `tauri-specta` generates TypeScript bindings from Rust commands and removes a whole class of type-drift bugs. It's still a release candidate (`2.0.0-rc.x`), so pin the version, keep a thin wrapper, and treat `taurpc` as the fallback |
| **UI primitives** | Add | React Aria Components: desktop-grade keyboard and focus behaviour for grids, menus, comboboxes and date fields. This matters more for an accounting app than visual polish does |
| **Data layer in UI** | Add | TanStack Query over IPC, plus TanStack Table + Virtual so a 100k-line ledger scrolls at 60 fps |
| **PDF** | Add | Typst, embedded as a Rust crate. Deterministic, template-based, excellent typography. Gap: PDF/A-3 with an embedded XML attachment (Factur-X/ZUGFeRD) isn't upstream yet, so hybrid PDFs are deferred |
| **E-invoice validation** | Add | Generate UBL 2.1 (Peppol BIS Billing 3.0) and CII natively. Validate against XSD and the official EN 16931 Schematron in **CI** using the KoSIT validator (Java, containerised). Implement the critical business rules natively for runtime checks |
| **Rule packs** | Add | Versioned TOML for *data* (rates, thresholds, deadlines, form mappings) plus **Rust modules** for computation per jurisdiction. **No homegrown DSL.** That classic trap costs a year. Consider sandboxed WASM community packs in v2 |
| **Testing** | Add | `proptest` for kernel invariants, `insta` snapshots for XML and PDF outputs, `cargo-fuzz` for every bank-file parser (untrusted input), Playwright against the mock-IPC web build, and an advisor eval set |
| **Claude via API (second driver)** | Add | There's no official Anthropic Rust SDK, so the API driver speaks raw HTTPS per the API docs. Default model `claude-opus-5-5` with effort set explicitly (its default is `medium`); `claude-sonnet-5-5` or `claude-haiku-4-5` for high-volume ranking. Use structured outputs: forced `tool_choice` returns 400 on current models |

---

## 4. LLM integration review

### 4.1 Is the CLI-sidecar design allowed? Yes, under conditions

From Claude Code's *Legal and compliance* documentation (October 2026):

- Third-party developers may not offer Claude.ai login in their own apps, may not route requests through Free, Pro or Max credentials on users' behalf, and may not collect, store or intermediate Claude credentials or session tokens.
- These restrictions do **not** prevent an end user from signing in to the **unmodified Claude Code binary** with their own Claude subscription, including where a product runs Claude Code.
- A product running Claude Code must not modify the binary or remove or restrict any of its sign-in methods. It may not pay for, resell or intermediate usage: each end user authenticates and is billed under their own agreement.
- Advertised Pro and Max usage limits assume ordinary, individual use.

**Design consequences, now hard requirements:**

1. sky-la detects a **user-installed** `claude` binary. It never bundles or patches one. (Bundling would mean the project itself running Claude Code in its product, under the Commercial Terms.)
2. sky-la never reads, copies or stores Claude credentials, and never shows a Claude login form. To connect, it opens the user's terminal to run `claude auth login` through Anthropic's own flow.
3. No billing intermediation of any kind.
4. Advisors are **event-driven and low-volume** (period close, import, explicit request), not background batch jobs.
5. No Claude, Claude Code or Anthropic names or logos in product or feature names. Plain-text "uses your Claude Code" is fine.
6. A second driver (API key, stored in the OS keychain) serves users without a subscription and serves gated evals in CI.

### 4.2 The launch profile, corrected

`--bare` would have been the natural choice for a scripted engine, but in bare mode Claude Code *never reads OAuth credentials or the keychain*, so subscription users can't use it. The hardened profile is:

```text
claude -p
  --restricted                      # no command/code tools, no WebFetch; only managed + --settings load
  --tools ""                        # no built-in tools at all
  --strict-mcp-config --mcp-config <run>/skyla-mcp.json   # only sky-la's MCP server
  --allowedTools "mcp__skyla__*"
  --permission-mode dontAsk --permission-prompts none
  --system-prompt-file <run>/system.md
  --output-format stream-json --verbose
  --json-schema <run>/output.schema.json
  --no-session-persistence --max-turns 8
  --model <user choice> --effort <per task>
cwd = an empty, per-run temp directory; environment scrubbed
```

**Still to be proven by the spike (WP-01):** that `--restricted` works with subscription OAuth, and what user context still loads (`~/.claude/CLAUDE.md`, auto memory). If anything leaks, add `--setting-sources` / `--settings` with an explicit empty configuration. Also: that `--tools ""` with MCP tools behaves as documented; real cold-start latency; and that the `total_cost_usd` and `structured_output` fields are present.

### 4.3 Where the egress boundary really is

Data leaves the machine in **three** places, not one:

1. The prompt and system prompt.
2. **Every MCP tool result.** When the model calls `get_entries`, the result goes to Anthropic.
3. The model's own context carried across turns.

So the consent gate and redaction must sit **inside the core**, at the one choke point every MCP call passes through. That is why the MCP server becomes a thin shim forwarding to the core over an authenticated local socket. As a bonus, the SQLCipher key never enters a second process.

### 4.4 Prompt-injection posture

Bank memos, counterparty names and invoice text are untrusted input that will reach the model. Blast radius is contained by construction:

- The model can only *propose*.
- Proposals are validated by the kernel and approved by a human.
- Imported text is fenced as data in tool results.
- Any number in advisor prose must trace to an engine value (numeric grounding).

---

## 5. Plan review

- **Reorder.** Scaffold → **CLI spike (timeboxed, throwaway)** → money + store → ledger kernel → shell. The spike's outcome can change the architecture (policy, flags, latency). The kernel is well-understood territory, so it can follow.
- **Make each packet fit one session.** Every work packet states its inputs, deliverables, *acceptance commands* and what is out of scope. A session that ends without green acceptance commands hasn't finished its packet.
- **Make the UI testable without a display.** Cloud sessions can't drive a Tauri window, but they *can* run the React app in Chromium against a mock IPC transport with fixture data. Playwright screenshots then let later sessions verify UI work visually. This one decision roughly doubles what each session can verify.
- **The Claude CLI in cloud sessions.** Advisor work is developed against recorded stream-json fixtures and a `Fake` provider. The spike records the fixtures. Live-model evals run only when explicitly invoked.

---

## 6. Design review

### 6.1 Why round 1 failed

- **Cosmetic variety.** All six showed the same composition (KPI tiles, a bar chart, an advisor card, a table) in different skins. There was nothing structural to choose between.
- **Dated typography strategy.** Identity came from display serifs and decorative monospace, a 2020–2022 web-trend vocabulary. Desktop-grade work needs system-native type at small optical sizes, with tabular figures in the *sans*.
- **Card-everything section design.** Bordered cards around every group, low information density, large hero headings: web-dashboard habits, not desktop ones.
- **Wrong centre.** A dashboard is the least important screen in accounting software.
- **Factual errors in our own sample copy, which proves the architecture point.**
  - The mockups conflated the 60% flat-rate ceiling (1 200 000 Kč applies to the *deduction*, i.e. 60% of 2 000 000) with a revenue cap.
  - They showed 448 740 as 60% of 1 178 300, which is actually 706 980.
  - They framed the flat-rate choice as "3 entries to post", when it's an election on the annual return.

  A model writing tax prose makes exactly these mistakes. That is why tax arithmetic lives in Rust and every number is grounded.

### 6.2 Round 2: four theses

Each direction answers *"what is the centre of this app?"* differently. All share one entity and one internally consistent dataset, and all mirror current macOS (Tahoe-era) space efficiency and component structure.

| | Thesis | Centre of the app | Shown on |
|---|---|---|---|
| **A — Tahoe** | It's a Mac app | Classic three-pane: floating glass source list, list, inspector | Invoices, with the invoice preview, QR Platba, dunning timeline and a late-payer insight |
| **B — Inbox** | Accounting is triage | One queue of decisions (deadlines, approvals, advice), keyboard-first | The AWS reverse-charge posting with its balanced journal entry and the rule that matched it |
| **C — Statement** | The books are a document | Statements whose every figure drills to its entries; advisor notes live in the margin | Q3 profit and loss, accrual vs cash (*daňová evidence*) toggle, inline drill-down |
| **D — Workbench** | Pro instrument | Dense split view for high-throughput work, dark appearance | Bank reconciliation: statement tie-out, explained match scores, posting and its effects |

The directions aren't mutually exclusive. A likely winner is **A's chrome + B's inbox as home + C's statements + D's workbench as the reconciliation screen**. The design question is really which one sets the *home screen and the visual language*.

---

## 7. Risk register (top 10)

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Anthropic changes CLI terms, flags or limits | Medium | High | Provider trait with API driver; spike pins behaviour; CI smoke test against the CLI version matrix; monitor docs |
| R2 | Wrong tax output harms a user | Medium | Very high | Deterministic engine, golden tests from official examples, numeric grounding, scenario framing, accountant package |
| R3 | Czech legislative churn (annual changes) | Certain | Medium | Effective-dated, versioned packs; a yearly pack-update work packet; citations embedded in each pack |
| R4 | Scope creep across jurisdictions | High | High | CZ pack complete before any other; pack interface proven by an EU-VAT module first |
| R5 | Data loss (passphrase forgotten, disk failure) | Medium | Very high | Recovery key, scheduled encrypted backups, restore tested in CI |
| R6 | Prompt injection via imported text | Medium | Medium | Propose-only tools, kernel validation, fencing, human approval |
| R7 | WebKitGTK quirks on Linux | High | Low–medium | Linux QA lane; avoid exotic CSS; feature-detect backdrop blur |
| R8 | `tauri-specta` stays release-candidate or breaks | Medium | Low | Pinned version, thin wrapper, `taurpc` fallback |
| R9 | Typst PDF/A-3 gap blocks Factur-X | High | Low | Hybrid PDFs deferred; UBL/CII XML satisfy EN 16931 on their own |
| R10 | Subscription usage limits throttle advisors | Medium | Medium | Low-volume event-driven design; per-run budget shown; API-key driver as alternative |

---

## 8. Sources checked

- Claude Code — [CLI reference](https://code.claude.com/docs/en/cli-reference): `--restricted`, `--tools`, `--strict-mcp-config`, `--json-schema`, `--permission-prompts`, `--no-session-persistence`, `--max-budget-usd`
- Claude Code — [Run Claude Code programmatically](https://code.claude.com/docs/en/headless): bare mode skips OAuth and the keychain; stream-json and `structured_output`
- Claude Code — [Legal and compliance](https://code.claude.com/docs/en/legal-and-compliance): authentication and credential use, running Claude Code in products, trademark use
- Výdajové paušály 2026 (60% → max 1 200 000 Kč; 80% → 1 600 000; 40% → 800 000; 30% → 600 000): [mesec.cz calculator](https://www.mesec.cz/kalkulacky/vydajove-pausaly/), [rulecalc.com](https://rulecalc.com/cs/osvc/pausalni-vydaje-osvc)
- Paušální daň 2026 bands: [iDoklad](https://www.idoklad.cz/blog/pausalni-dan-2023-jaka-jsou-pasma-a-vyse-dane), [skrblik.cz](https://www.skrblik.cz/rodina/dane-a-statni-podpora/pausalni-dan)
- Typst ZUGFeRD / PDF/A-3 status: [Typst forum thread](https://forum.typst.app/t/zugferd-electronic-invoices-with-typst/1019)
- tauri-specta Tauri v2 status: [docs.rs/tauri-specta](https://docs.rs/tauri-specta)
- macOS Tahoe design language (Liquid Glass, concentricity, floating sidebar): [WWDC25 session 310](https://developer.apple.com/videos/play/wwdc2025/310/)

Every statutory value used in code must come from a rule pack with its own citation. The sources above justify the review's corrections; they aren't a substitute for primary legislation.
