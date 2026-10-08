# Import samples (synthetic)

Hand-written files in the shapes Pohoda (XML data pack) and Fakturoid (CSV
export) produce, for the importer's tests and the demo's import preview.
They are synthetic: the customers are the demo's invented companies and no
file was exported from a real installation.

- `pohoda-faktury.xml`: one invoice dated before the demo books begin, one
  already in the books (2026-041), two new ones, one whose VAT doesn't match
  the rate, and a received invoice the importer skips.
- `fakturoid-faktury.csv`: totals only (the importer finds the rate from
  the pack), semicolon-separated with decimal commas, UTF-8 with a BOM; the
  last row is a credit note.
- `pohoda-dobropisy.xml`: credit notes against the demo's open invoice
  2026-102 (one partial, one that exceeds what is then still open, stated
  with positive amounts), against the paid 2026-114 (nothing open), against
  a missing invoice, and one listed before its own invoice 2026-130 in the
  same file.
- `fakturoid-dobropisy.csv`: a "Typ" and a "Původní doklad" column; an
  invoice, a partial credit note of it, one over what is left, and one
  against the paid 2026-041 dated before the books begin.
