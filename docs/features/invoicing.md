# Invoicing

Issued invoices, credit notes and advance documents, from draft to PDF and e-invoice, plus recurring invoices and payment reminders.

**Code:** `crates/skyla-invoicing`, `crates/skyla-render`, `crates/skyla-app/src/core.rs` (drafts, issue, export) and `core/recurring.rs` · **UI:** `apps/desktop/src/screens/{Invoices,InvoiceEditor,Reminders}.tsx` · **Tests:** `crates/skyla-invoicing/tests`, `crates/skyla-render/tests`, `crates/skyla-app/tests/{drafts,recurring,reminders}.rs`

## Documents and lifecycle

- **Drafts change freely.**
  - The editor (`#/invoices/new`) collects what was typed: customer (or **New customer…**), payment terms, lines (description, quantity, unit, price without VAT, VAT code) and a note.
  - The core parses Czech amounts and quantities, checks everything and lists every problem at once. The editor marks each problem on its field.
  - **Edit** on a draft reopens it in the same editor (`#/invoices/edit-<id>`); changes are checked like a new draft.
- **Issuing:**
  - assigns a gapless number in the series (patterns like `{YYYY}-{NNN}`), in the same transaction that posts the entry;
  - fixes the VAT recapitulation with the pack's rate and rounding on the tax point;
  - posts receivables, revenue and VAT through the kernel.
- **Issued documents are frozen** (triggers too). Corrections are credit notes, which settle the invoice they correct, so open amounts and the cash basis stay right. Over-crediting is refused.
- **Advances:** advance invoices, the tax document on a received advance and the final invoice's deduction follow Czech practice. The VAT return shows each part in its own month.
- **Supplier snapshot:** the supplier profile (IČO checksum, DIČ, IBAN) is snapshotted onto each document at issue, so a later change never rewrites an issued invoice.

## Output formats

| Format | Notes |
|---|---|
| PDF (cs/en) | Typst, compiled in memory (no file system, network or clock), so the same document renders to the same bytes. Rust prepares every printed string. Includes the **QR Platba** (SPAYD 1.0) code when there's something to pay. |
| ISDOC 6.0.2 | Document types 1, 2, 4, 5. Validated against the official XSD in tests. |
| UBL 2.1 (Peppol BIS 3.0) | Validated against the XSD, the CEN EN 16931 Schematron and the OpenPeppol BIS 3 Schematron (`just einvoice`). |
| CII D16B (EN 16931) | For Factur-X and ZUGFeRD. Validated the same way. |

## Recurring invoices

- In the editor, **Repeat** (monthly, quarterly, yearly; a first date; a template name; optionally issue automatically) makes a template.
- Runs are idempotent per occurrence. A run catches up in calendar order after the app was closed, never backdates, and clamps month-ends (including 29 February). `{month}`, `{MM}` and `{YYYY}` in texts become the occurrence's values.
- Real books run what's due when a template is created and whenever they open. **Recurring** on the Invoices toolbar lists templates and pauses or resumes them.

## Payment reminders (dunning) and late interest

- The default policy is a three-step sequence at 3, 14 and 30 days overdue. The user can change it.
- Each step goes out at most once, on a working day. Payment or a hold stops the sequence. A missed step is skipped rather than sent after a firmer one.
- The core drafts the reminder in Czech and English with what's owed, the account and the variable symbol.
- **Statutory late interest:** the ČNB repo rate on the first day of the half-year the delay began, plus the pack's margin. It accrues daily (actual/365) and splits at partial payments.
- An overdue invoice's inspector shows the reminder that's due, with **Copy text** and **Mark as sent**. `record_reminder` records only the due step, once; `reminders_sent` lists what went out.
- The app sends no email (the network invariant). The user sends the reminder from their own mail.

## Not yet

- Holding reminders for a disputed invoice (the kernel has `hold_reminders`; there's no UI).
- A tax point other than the issue date, a buyer reference or a discount in the editor.
- Refunds for credit notes against a paid invoice.
- Multi-currency invoices.
- Saving PDFs through a native save dialog (the desktop uses a blob download).
- An EU-supply VAT code; the souhrnné hlášení needs it first.
