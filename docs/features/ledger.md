# Ledger kernel

The double-entry core every other module posts through. It's jurisdiction-neutral and Apache-2.0 licensed, and it never depends on the AGPL crates.

**Code:** `crates/skyla-money`, `crates/skyla-ledger` · **Tests:** `crates/skyla-money/src` (unit and property), `crates/skyla-ledger/tests` (`posting`, `close`, `chart`, `projections`)

## Money (`skyla-money`)

- `Money` is an `i64` count of the currency's minor unit (haléř, cent) with an ISO 4217 table. Arithmetic is checked and never panics.
- Anything fractional (rates, VAT, FX) uses exact decimals and an explicit `RoundingMode`: half-even, half-up, toward zero or away from zero.
- `vat::from_base` and `vat::from_gross` guarantee base + VAT = gross. `allocate` splits an amount exactly.
- Czech formatting and strict parsing: `84 700,00`. Ambiguous input such as `1.234` is refused rather than guessed.

## Accounts and the chart

- A chart arrives as data (`ChartSpec`, e.g. `rules/cz/chart.toml`: 63 synthetic accounts per vyhláška 500/2002 Sb. and 22 freelancer categories). A validator reports every problem at once.
- SQL triggers keep the structure sound:
  - top-level accounts have three digits;
  - analytic codes extend their parent and inherit its kind and side;
  - structure is immutable, and accounts are deactivated, never deleted.

## Posting and the invariants

Each invariant is enforced twice: in Rust (typed errors) and in SQL triggers, so raw SQL can't bypass it.

| | Invariant |
|---|---|
| I1 | Every entry balances in the functional currency. |
| I2 | Posted entries and postings are frozen and never deleted. |
| I3 | An open period covers the entry's date. |
| I4 | Lines target active leaf accounts, re-checked at post time. |
| I7 | Rule and advisor entries need a human approver. |

Other rules the kernel keeps:

- Posted entries get a gapless `posted_seq`.
- FX lines carry their rate, with a functional amount of the same sign.
- The functional currency is fixed once anything is booked.
- Operations use savepoints, so they nest inside a caller's transaction.
- `check_entry` runs an entry through every rule and always rolls back. Advisor tools use it to check proposals.

## Periods, reversals and the hash chain

- Periods move from open to closing (adjustments only) to closed (final). A close records who, when and the chain head.
- `CloseCheck`s are pluggable; the report lists every failure.
- `reverse_entry` posts a mirror entry linked by `reverses_id`, at most once per entry. It reverses settlement links too, so a reversed payment reopens the invoice it settled.
- Every posted entry extends a SHA-256 hash chain computed in the posting transaction. `verify_chain` names the first missing, altered or seal-inconsistent entry.
- The chain head is also anchored outside the live file, in backup manifests and the export manifest.

## Reports (projections)

- Trial balance, P&L and balance sheet by synthetic account.
- Cash basis (*daňová evidence*) from settlement links and cash-flagged accounts.
- VAT ledger by form row, from pack-supplied mappings.

Every report carries an input-snapshot hash. The golden journal (`packages/fixtures/data/demo-ledger.json`) reproduces the design canvas's figures to the cent. A trial balance over 100 000 entries takes about 0.1 s in release (`just bench`; the target is under 200 ms).

## Replays

`create_draft_as`, `post_entry_at` and `reverse_entry_at` take a known uid and posting time. The demo and the IPC recordings use them, so the hash chain comes out the same on every run.

## Not yet

- Reversing into an account deactivated since the original was posted fails on I4. The UI doesn't yet offer to reactivate the account.
- Multi-currency documents: the books record FX lines, but invoices are in the functional currency only.
