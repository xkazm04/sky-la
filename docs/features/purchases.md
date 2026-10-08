# Purchases (received invoices)

Supplier invoices: what's owed, what's paid, and the VAT that can be deducted.

**Code:** `crates/skyla-app/src/core/purchases.rs` · **UI:** `apps/desktop/src/screens/Purchases.tsx` · **Tests:** `crates/skyla-app/tests/purchases.rs`

## What it does

- The **Purchases** screen lists received invoices, grouped as *to pay* and *paid*, with days overdue.
- **Record a received invoice** takes:
  - the supplier, IČO and DIČ;
  - the invoice number and the issue, tax-point and due dates;
  - one line per expense account (base, VAT code);
  - optionally the VAT the invoice states.
- The core checks it all and lists every problem:
  - a DIČ is required to deduct VAT;
  - the date must fall in an open period;
  - the same invoice can't be recorded twice;
  - the VAT the pack computes on the tax point must match the VAT the invoice states, if typed.
- It posts one entry (Dr 5xx with an input VAT code, Dr 343, Cr 321) approved by the user, and keeps the supplier in the `received_invoice` table.

## How other modules use it

- **Bank:** an open 321 payable is an open item. The matcher pays it by its variable symbol and amount, and the payment settles it through settlement links.
- **Kontrolní hlášení:** recorded suppliers' DIČs itemise section B.2.
- **Advisors:** suppliers are pseudonymised by the egress gate like customers. The financial detectors attribute expenses to suppliers.

## Not yet

- Importing received invoices from Pohoda or Fakturoid (only issued invoices import).
- Purchases above the fixed-asset threshold wait for the depreciation groups in the pack.
