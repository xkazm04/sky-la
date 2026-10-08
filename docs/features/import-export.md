# Import and export

Moving from another tool, and taking everything out again.

**Code:** `crates/skyla-invoicing/src/import.rs`, `crates/skyla-app/src/core/{imports,export}.rs` · **UI:** Invoices → **Import…** (`InvoiceImport.tsx`), Settings → **Export everything** · **Samples:** `packages/fixtures/data/imports/` · **Tests:** `crates/skyla-app/tests/{imports,export}.rs`

## Import issued invoices and credit notes

- **Sources:**
  - **Pohoda** XML data packs. Elements are matched by local name, so either envelope works. DTDs are refused. Per-rate totals come from `invoiceSummary/homeCurrency`.
  - **Fakturoid** CSV, with Czech or English headers, semicolon or comma, decimal comma or point, and UTF-8 or Windows-1250.
- **Preview:** each document is marked *new*, *already in these books* (by number) or *won't import*. Reasons for refusal:
  - outside the kept periods, or in a closed one;
  - VAT that doesn't match the pack's rate on the tax point;
  - an invalid IČO;
  - another currency;
  - a supply without VAT from a VAT payer;
  - a credit note that can't be tied to its invoice (see below).

  A totals-only row takes whichever pack rate reproduces its VAT exactly.
- **Commit:** the new documents post in one savepoint through the kernel (Dr 311 / Cr revenue and VAT). They're kept as imported documents in a matching series, or a new `IMP<n>` series inferred from their numbers.

### Credit notes

- **Reading:** Pohoda `issuedCreditNotice` with the original in the header's `sourceDocument` (its `number`, `numberRequested` or `ids`); Fakturoid rows whose type says *oprav…*, *dobropis* or *credit*, or that carry an original-invoice column (*Původní doklad*, *Číslo původní faktury*, *Original invoice*, …), or whose total is negative. Programs disagree on the sign, so the reader keeps every credit note the way sky-la issues its own: base, VAT and total negative. A credit note whose amounts mix signs is skipped with a message.
- **Order:** originals first. The preview judges every invoice of the file before any credit note, wherever they sit in the file, and the commit posts the new invoices, then the new credit notes (each in file order).
- **New only when:** the invoice it names is among the issued invoices in the books, or is a new invoice in the same file; it is for the same customer (IČO) and not dated before it; the credit note itself passes the same checks as an invoice (open period, VAT matches the pack's rate on its tax point); and it doesn't exceed what is still open on the invoice (gross less payments and earlier credits, credit notes of the same file included). That is the cap the ledger enforces on the settlement link, so a refusal in the preview is never a failure at commit.
- **Won't import, with the reason:** no original named, original not found (or found in the file but refused itself), more than is open, another customer, dated before the invoice, outside the kept periods or in a closed one, VAT that doesn't match.
- **Posting:** the reverse of an invoice (Dr 602 / Dr 343 / Cr 311), approved by the user, linked as a settlement of the invoice's entry, and recorded as an imported credit-note document pointing at the invoice, in a matching `credit_note` series (the demo's `OD`, say) or a new `IMP<n>` one.
- **Already here:** by number, as for invoices.

## Export everything

- One reproducible zip:
  - the journal as JSON and CSV;
  - the chart of accounts;
  - every issued document as ISDOC and PDF;
  - a manifest with the chain head and the SHA-256 of each file.
- The export asks for the passphrase again before it starts. File names are ASCII-folded so every browser and file system keeps them (`sky-la-export-Jan_Novak-2026-10-07.zip`).

## Not yet

- Importing received invoices into Purchases.
- Per-line detail from Pohoda's `invoiceDetail`.
- Foreign-currency invoices.
- Bank statements and the egress register in the export.
