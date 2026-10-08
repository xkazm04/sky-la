# Taxes

Everything on this screen is computed by sky-la's engine from your books and the rule pack. It is a draft for your review, not a filing. The pack's status (currently *draft*) is shown beside it. sky-la doesn't file anything: it prepares the figures, and you file them through the tax portal (EPO) or with your adviser.

## VAT (DPH) returns

For each VAT period, the return's rows (as on form DPHDP3) are filled from your postings' VAT codes, with the tax payable or the excess deduction and the filing deadline. A VAT code the pack doesn't map is listed as **unmapped**, and the return isn't complete until you fix it.

## Kontrolní hlášení

Sections A.2, A.4, A.5, B.2, B.3 and C, from your issued and received invoices. Documents above the pack's itemisation threshold are listed one by one, with the counterparty's DIČ. A purchase without one, above the threshold, is flagged for you to fix. Section C is checked against the DPH return for the same period.

## Income tax and insurance scenarios

**Income tax · daňová evidence** builds the § 7 worksheet from your books: income, deductible and non-deductible expenses, and the tax base. **Actual or flat-rate expenses** compares your real expenses with the flat rate for your group (capped by the pack), and shows the income tax, social and health insurance, and the pension assessment base for each. You can project the rest of the year with your own assumptions. The scenarios say what each choice costs; they don't choose for you.

Some levers wait for pack values that aren't in yet, such as paušální daň for 2026 and depreciation groups. They say "not assessed" rather than guess.

## The obligations calendar

**Obligations calendar** lists every deadline that applies to you: the DPH return and the kontrolní hlášení, the income-tax return (on paper or electronically), and the social and health insurance advances. Each is moved to the next working day where the law says so, and cites its provision.

## Asking the tax advisor

**Explain with the tax advisor…** sends the scenarios (not your books) to Claude through the egress gate, after saying exactly what will be sent. The answer is accepted only if its recommendation is one of the engine's scenarios and every number in it matches an engine or pack value. It lands in your inbox as a scenario for your review.
