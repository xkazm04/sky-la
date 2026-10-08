# Advisors

AI advisors run through the user's own Claude Code CLI. They **propose, explain and calculate; they never post.** What they're sent passes an egress gate and is recorded, and figures in their prose must match the engine's.

**Code:** `crates/skyla-advisor` (provider, CLI driver, Fake driver, stream parser, grounding), `crates/skyla-mcp` (the MCP shim), `crates/skyla-egress` (gate, pseudonyms, register), `crates/skyla-app/src/core/{advisor,tools,toolhost,egress,findings,explain}.rs` · **UI:** `apps/desktop/src/screens/{Advisors,AdvisorConnection,ExplainThis,Register,TaxScenarios}.tsx` · **Tests:** `crates/skyla-advisor/tests`, `crates/skyla-mcp/tests`, `crates/skyla-egress/tests`, `crates/skyla-app/tests/{tools,toolhost,egress,tax_advisor,findings,threats}.rs`

## The provider

- `LlmProvider` has two jobs: availability and run. A run folds the stream into an outcome and classifies how it ended: completed, not signed in, rate-limited (with the reset time), interrupted, or failed.
- **`ClaudeCodeCli`** launches the user-installed, unmodified `claude` with a hardened profile:
  - fixed argv, `--disable-slash-commands`, and the answer shape through `--json-schema`;
  - an environment rebuilt from an allow-list (no API keys, tokens or the parent's session id);
  - an empty per-run directory, and stdin closed.
- The driver checks the `init` event against the profile and kills the run on any violation before a tool can run.
- Sign-in happens in the user's terminal (`claude auth login`). The app never reads, stores or proxies Claude credentials.
- **`Fake`** replays recorded transcripts. All development, CI and the demo use it, and contract tests run the same transcripts through both drivers.

## Tools (MCP)

- The run's only MCP server is `skyla-mcp`, a stdio shim that links only `serde_json`, holds no data and writes nothing.
- The shim forwards `tools/list` and `tools/call` to the core's tool host on a loopback port. Each run gets a new port and a fresh token, checked in constant time and refused once the run ends.

| Kind | Tools |
|---|---|
| Read | `get_period_summary`, `get_vat_return`, `get_cash_basis`, `list_unmatched_bank_lines`, `get_rule_value` (with citation), `list_obligations` |
| Compute | `run_scenario` |
| Propose | `propose_entry` (kernel-checked), `propose_categorisation` (kernel-checked), `propose_finding` (must cite), `ask_user` |

A test calls every tool and shows the journal, invoices, bank state and chain head unchanged. Proposals wait in the [Inbox](inbox.md).

## The egress gate and the register

- Every prompt **and every tool result** passes the `Gate` before reaching the model.
- **Always withheld:** IBANs, domestic account numbers, personal ID numbers and card numbers. The gate over-redacts by design.
- Customer and supplier names become stable pseudonyms ("Customer A") unless the task's scope grants names, and they're revealed again locally.
- Fields outside the task's scope are dropped.
- **Task policies** cover four tasks: `tax.scenarios`, `financial.findings`, `explain.figure` and `bank.categorise`. Each is set to ask (the default), always or never, in Settings → Advisor sharing.
- **The register** is an append-only, hash-chained table of every run, with the exact bytes sent and the tool results. **Egress register** in the app shows "What was shared".

## Grounding

`skyla_advisor::grounding` reads figures the Czech way (`942 600 Kč`, `29,2 %`). It flags every figure that isn't an engine value, a tool result's value or a statutory value from the pack. A tax-advisor answer with one figure changed by a crown is rejected and neither shown nor filed.

## What the advisors do

- **Tax advisor:** computes the § 7 scenarios with the engine and asks the model to explain them. The answer is accepted only if the recommendation is one of the engine's scenarios and every figure is grounded. Accepted answers go to the inbox as a scenario for review.
- **Financial advisor detectors:** pure functions over the books. They don't need the model to find anything, only to explain:
  - subcontracting period on period;
  - hourly rate per supplier;
  - margin per customer;
  - late payers;
  - subscription creep and duplicated charges;
  - runway.
- **"Explain this":** on a figure (an account for a period, or an entry), the model explains the movement. The answer must cite only entries behind the figure.

## Evals

- 30 Czech eval cases pass on `Fake`.
- `just eval-live` runs them on demand through the user's own CLI and writes a report with cost estimates. It refuses to run under CI.

## Not yet

- **Blocked:** the `AnthropicApi` driver (D-005). It would add a network path and a stored API key that the invariants don't allow. The invariant must be amended or the driver dropped.
- Check C6 of the CLI spike (signed out, desktop keychain) must run on a real desktop.
