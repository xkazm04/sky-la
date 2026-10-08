# Import and export

Moving from another tool, and taking everything out again.

**Code:** `crates/skyla-invoicing/src/import.rs`, `crates/skyla-app/src/core/{imports,export}.rs` · **UI:** Invoices → **Import…** (`InvoiceImport.tsx`), Settings → **Export everything** · **Samples:** `packages/fixtures/data/imports/` · **Tests:** `crates/skyla-app/tests/{imports,export}.rs`

## Import issued invoices

- **Sources:**
  - **Pohoda** XML data packs. Elements are matched by local name, so either envelope works. DTDs are refused. Per-rate totals come from `invoiceSummary/homeCurrency`.
  - **Fakturoid** CSV, with Czech or English headers, semicolon or comma, decimal comma or point, and UTF-8 or Windows-1250.
- **Preview:** each document is marked *new*, *already in these books* (by number) or *won't import*. Reasons for refusal:
  - outside the kept periods, or in a closed one;
  - VAT that doesn't match the pack's rate on the tax point;
  - an invalid IČO;
  - another currency;
  - a supply without VAT from a VAT payer;
  - a credit note.

  A totals-only row takes whichever pack rate reproduces its VAT exactly.
- **Commit:** the new documents post in one savepoint through the kernel (Dr 311 / Cr revenue and VAT). They're kept as imported documents in a matching series, or a new `IMP<n>` series inferred from their numbers.

## Export everything

- One reproducible zip:
  - the journal as JSON and CSV;
  - the chart of accounts;
  - every issued document as ISDOC and PDF;
  - a manifest with the chain head and the SHA-256 of each file.
- The export asks for the passphrase again before it starts. File names are ASCII-folded so every browser and file system keeps them (`sky-la-export-Jan_Novak-2026-10-07.zip`).

## Not yet

- Importing credit notes against their imported invoice.
- Importing received invoices into Purchases.
- Per-line detail from Pohoda's `invoiceDetail`.
- Foreign-currency invoices.
- Bank statements and the egress register in the export.
