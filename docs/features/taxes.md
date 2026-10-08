# Taxes

Czech returns computed from the books, the income-tax scenarios and the obligations calendar. Every rate, row and threshold comes from the rule pack. The UI calls the results drafts and scenarios for the user's review, not tax advice.

**Code:** `crates/skyla-tax-cz` (`kh.rs`, `sh.rs`, `income.rs`), `crates/skyla-app/src/core/{tax,periods}.rs`, `skyla_rules::calendar` · **UI:** `apps/desktop/src/screens/{Taxes,TaxScenarios,Calendar}.tsx` · **Tests:** `crates/skyla-app/tests/{control_statement,recapitulative,tax_scenarios,periods}.rs`, `crates/skyla-tax-cz/tests`

## DPH return

- The VAT ledger maps each posting's VAT code to its form row through the pack.
- The return covers a period's output and input VAT and the amount to pay or reclaim, with reverse charge and advances in their own months.

## Kontrolní hlášení

`control_statement` sorts a period's documents into the sections:

| Section | Contents |
|---|---|
| A.4 / A.5 | Supplies to VAT payers above / below the pack's itemisation threshold (10 000 Kč including VAT; strictly above). |
| B.2 / B.3 | Purchases with the supplier's DIČ above / below the threshold. A purchase above it without a DIČ is reported, not guessed. |
| A.2 | Services received from the EU under reverse charge, always itemised, with their tax counted once. |
| C | Totals of the return's rows, checked against the DPH return. |

A correction follows the section of the document it corrects.

## Souhrnné hlášení

`recapitulative_statement` (`skyla_tax_cz`; `Core::recapitulative_statement`) lists a period's supplies to VAT payers in other member states from the issued documents:

- one line per customer VAT number and supply code, with the total base and the number of supplies;
- which VAT codes count, and the statement's code for each (`0` goods, `3` services), come from the pack's `eu_supply`;
- a credit note follows the invoice it corrects into that invoice's period, so the statement shows what the books now say about it; a customer whose supplies net to nothing drops out;
- a document without a usable customer VAT number is listed under problems, not guessed.

The kontrolní hlášení leaves these supplies out. They feed ř. 20 and ř. 21 of the DPH return, which carry a base and no tax.

## Income tax: the § 7 worksheet and scenarios

- **The worksheet** takes § 7 income and expenses (actual, or a flat rate capped on the deduction) to:
  - the tax base, rounded down to hundreds;
  - 15 % less the taxpayer credit (never below zero), with the tax rounded up to crowns;
  - social and health insurance from the same profit.

  It says which omitted pack values it couldn't apply.
- **The scenario engine** enumerates the levers the facts allow and computes each:
  - the expense method;
  - buying a planned purchase this year or next, when it's below the asset threshold.

  It reports the differences in tax and insurance and which scenario is lowest. The design review's worked example reproduces exactly.
- The paušální daň lever reports "not assessed" until the pack holds the 2026 bands.

## Obligations calendar and deadlines

- `Pack::calendar` expands the pack's obligations for a year, shifted past weekends and holidays.
- `obligations(year)` marks each deadline past, next or upcoming, with its act and provision.
- The next 31 days' deadlines become [Inbox](inbox.md) items, one per period and due date, so a DPH return and its KH due together are one item. Each item explains a shifted date.

## Reporting periods

`reporting_periods()` derives what every screen reports on from the books' own date and VAT status:

- the last three ended VAT periods (monthly or quarterly; none for non-payers);
- the last two ended quarters;
- the year to date;
- the previous quarter for comparison.

No screen assumes the demo's dates.

## Not yet

- **Souhrnné hlášení:** no screen or Tauri command yet; its filing period and deadline and the EPO form's rounding aren't in the pack, so it isn't in the calendar.
- **EPO XML writers** (DPHDP3, DPHKH1, DPHSHV): the official schemas couldn't be fetched to prove the output XSD-valid.
- Minimum assessment bases, the solidarity threshold and insurance-overview deadlines (pack data missing).
