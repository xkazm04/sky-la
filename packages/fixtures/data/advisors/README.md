# Advisor transcripts for the demo

Synthetic, hand-written `stream-json` transcripts in the format Claude Code
emits (see `crates/skyla-advisor/fixtures/cli/`). The demo's `Fake` provider
answers with them; no model is called. Every figure in them was copied from
the engine's output for the demo books, so they pass the numeric-grounding
check; the eval set (`crates/skyla-app/tests/tax_advisor.rs`) also checks that
changing any one figure is caught.

| File | Answers |
|---|---|
| `tax-books.jsonl` | The tax advisor on the books so far (no projection) |
| `tax-review-projection.jsonl` | The tax advisor on the design review's projection (1 571 000 Kč income, 60 % flat rate, a laptop to time) |
