# sky-la

**Accounting that stays on your machine.** Open-source, encrypted, desktop-native books for Czech freelancers and micro-companies, with AI advisors that explain and suggest, but never touch your ledger without you.

> Status: **design phase.** No code yet. Start with the documents below.

| Document | What it covers |
|---|---|
| [`docs/design/DESIGN.md`](docs/design/DESIGN.md) | Product, scope, architecture, security, advisors |
| [`docs/design/REVIEW.md`](docs/design/REVIEW.md) | Critical review of the concept, stack and plan, and the corrections made |
| [`docs/design/DECISIONS.md`](docs/design/DECISIONS.md) | Every decision so far and the open questions |
| [`docs/plan/IMPLEMENTATION_PLAN.md`](docs/plan/IMPLEMENTATION_PLAN.md) | Work packets, milestones, acceptance criteria |
| [`docs/plan/STATUS.md`](docs/plan/STATUS.md) | Progress tracker |

## Principles

1. Correct by construction: integer money, immutable posted entries, invariants in the database itself.
2. Nothing posts without you.
3. Every number explains itself.
4. Deterministic first; AI only for what code can't decide.
5. Private by default; every byte shared with an AI is logged and replayable.

## AI advisors

sky-la can use **your own Claude Code installation** for its tax and financial advisors. You sign in to Claude Code yourself, and sky-la never sees your credentials. An Anthropic API key is the alternative. Advisors run without any network access of their own: they read through a narrow, audited interface and can only *propose*.

## Licence

To be decided (see `DECISIONS.md` Q-05).
