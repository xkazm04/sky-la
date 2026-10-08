# Advisors

sky-la's advisors use **your own Claude Code installation**. sky-la never sees, stores or asks for your Claude credentials, and it doesn't sell or proxy access. Usage counts against your own Claude plan.

## Connecting

1. Install Claude Code ([claude.com/claude-code](https://claude.com/claude-code)).
2. In a terminal, run `claude auth login` and sign in.
3. In sky-la, **Advisors** shows the connection: the version found, or what's missing (not found, not signed in, usage limit reached and when it resets).

The books work as usual without it; only the advisors wait.

## What advisors can and can't do

Advisors **read** through a narrow set of tools (a period summary, the VAT return, cash-basis figures, rule values, obligations, unmatched bank lines), **compute** through the scenario engine, and **propose**: entries, categorisations, findings, and questions for you. Proposed entries are checked by the ledger kernel (balanced, open period, leaf accounts) and wait in your inbox. **No advisor can post, issue, pay or delete anything.**

Each run gets its own empty working folder, a minimal environment and a one-time key to sky-la's local tool service. Claude Code's own file, shell and web tools are switched off for it.

## What is shared

Before anything leaves your computer, the egress gate:

- **withholds** IBANs, account numbers, personal ID numbers (rodná čísla) and card numbers, whatever a task may share;
- **replaces** customer and supplier names with stable pseudonyms ("Customer A", "Vendor B"), and puts the real names back in the answer on your computer;
- **drops** fields outside the task's scope.

**Settings → What advisors may send** sets each task to *ask before each run* (the default), *run without asking*, or *never run*.

## The egress register

**Egress register** lists every run: when, which task, which model, what scopes, the reported cost, and **What was shared**: the exact bytes that left, prompt and tool results alike. The register is append-only and hash-chained, so a deleted or edited entry shows.
