# Bank

Statements in, payments matched, every booking explained, approved by the user and reversible.

**Code:** `crates/skyla-bank`, `crates/skyla-app/src/core/bank.rs` · **UI:** `apps/desktop/src/screens/Bank.tsx` · **Tests:** `crates/skyla-bank/tests` (parsers, corpus, matching), `crates/skyla-bank/fuzz`, `crates/skyla-app/tests/bank.rs`

## Parsers

| Format | Notes |
|---|---|
| ISO 20022 camt.053 | Versions 001.02 – 001.08. DTDs refused, with a node limit and a 64-level nesting limit. |
| SWIFT MT940 | Continuation lines, the SWIFT envelope, several statements per file, reversals, `:86:` subfields or tags. |
| ABO/GPC | The Czech fixed-width export, in Windows-1250. |
| CSV | Through column profiles; Fio is built in. |

Every parser takes untrusted bytes and returns a statement or an error; none panics. Five fuzz targets (about 9.3 M runs) and a corpus replay hold them to that. Amounts are parsed exactly into minor units.

## Normalise, tie out, dedupe

- Accounts are canonicalised, so a Czech IBAN and `prefix-number/bank` compare equal. Names are compared without diacritics or legal forms.
- **Tie-out:** opening + lines = closing, and the opening must match the previous statement or the books. A failure names the difference and imports nothing.
- **Dedupe:** a file seen before is refused, and overlapping re-imports are recognised. Two identical card payments on one day stay two lines.

## The explainable matcher

`suggest` scores each candidate as a sum of named integer signals, and the UI shows the contributions. The signals:

- variable symbol, a one-digit VS typo, the invoice number in the message;
- exact amount, a split that adds up, partial or over-payment;
- a known payer account, the name, the date window.

A candidate needs at least one strong signal. Splits search a customer's oldest open invoices. On the simulated year, 85.8 % of lines are accepted automatically and none wrongly.

**User rules** match on account, name, message, symbols, direction and amount. A rule either books to an account (with a VAT code) or settles a customer, and can auto-accept.

## The reconciliation workbench

Each line is *certain*, *needs you* or *booked*.

| Action | What it does |
|---|---|
| **Accept N certain** | Books every certain line, each as one kernel entry approved by the user. Receipts settle issued invoices against 311; payments settle received invoices against 321, with settlement links so the cash basis follows. |
| **Accept** | Books one certain line. |
| **Book…** | Books account rows, splitting input VAT from each row's gross with the pack. |
| **Create rule…** | Makes a rule from the line (counter-account, else name, else message) and books the line by it. |
| **Undo booking…** | Posts a reversal entry, dated like the booking unless that period is closed. Settled invoices reopen and the line returns to its suggestion. A later booking of the same line gets a new identity. |

Lines that need a person and have no proposal appear in the [Inbox](inbox.md). Proposals about a line leave the inbox once it's booked. The workbench state persists in real books.

## Not yet

- Real (anonymised) statement samples from each major Czech bank; the samples are synthetic.
- CSV profiles beyond Fio, and a profile editor.
- Booking incoming lines to accounts in the UI; incoming lines settle invoices.
- Using camt.053's foreign amount and rate (`AmtDtls`) for card payments.
