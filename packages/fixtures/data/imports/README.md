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
