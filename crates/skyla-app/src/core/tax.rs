//! The kontrolní hlášení from the books (WP-21): every posted entry with
//! VAT in the period becomes a document, with the counterparty from the
//! invoicing module (supplies) or the purchase records (received invoices),
//! and section C is checked against the DPH return for the same period.
//! The § 7 scenarios (WP-22) start from the books or the user's projection.

use std::collections::HashMap;

use skyla_ledger::{cash_basis, list_posted};
use skyla_money::Money;
use skyla_tax_cz::{
    Expenses, FlatRate, KhDocument, KhItem, KhPart, KhSide, KhTotals, PlannedPurchase,
    ScenarioFacts, Section7, control_statement, scenarios,
};

use super::{Core, money};
use crate::dto::{
    ControlStatementDto, KhCRowDto, KhItemDto, KhTotalsDto, ObligationDto, TaxProjectionDto,
    TaxScenarioDto, TaxScenariosDto,
};
use crate::error::CoreError;

const VAT_ACCOUNT: &str = "343";

fn item_dto(i: KhItem) -> Result<KhItemDto, CoreError> {
    let base = i.base_standard.checked_add(i.base_reduced)?;
    let tax = i.tax_standard.checked_add(i.tax_reduced)?;
    Ok(KhItemDto {
        base: money(base)?,
        tax: money(tax)?,
        number: i.number,
        counterparty: i.counterparty,
        vat_id: i.vat_id,
        date: i.date,
        base_standard: money(i.base_standard)?,
        tax_standard: money(i.tax_standard)?,
        base_reduced: money(i.base_reduced)?,
        tax_reduced: money(i.tax_reduced)?,
    })
}

fn totals_dto(t: KhTotals) -> Result<KhTotalsDto, CoreError> {
    Ok(KhTotalsDto {
        base_standard: money(t.base_standard)?,
        tax_standard: money(t.tax_standard)?,
        base_reduced: money(t.base_reduced)?,
        tax_reduced: money(t.tax_reduced)?,
        documents: t.documents,
    })
}

impl Core {
    /// The control statement for `from..=to`, checked against the return.
    pub fn control_statement(
        &self,
        from: &str,
        to: &str,
    ) -> Result<ControlStatementDto, CoreError> {
        // Issued documents by their ledger entry.
        let mut issued: HashMap<i64, skyla_invoicing::Document> = HashMap::new();
        {
            let db = self.db();
            let ids: Vec<i64> = db
                .prepare("SELECT id FROM document WHERE entry_id IS NOT NULL")
                .map_err(skyla_ledger::LedgerError::from)?
                .query_map([], |r| r.get(0))
                .map_err(skyla_ledger::LedgerError::from)?
                .collect::<Result<_, _>>()
                .map_err(skyla_ledger::LedgerError::from)?;
            for id in ids {
                let d = skyla_invoicing::get(&db, &self.pack, id)?;
                if let Some(entry) = d.entry_id {
                    issued.insert(entry, d);
                }
            }
        }
        let entries = list_posted(&self.db(), from, to)?;
        let zero = Money::zero(self.currency);
        let mut docs = Vec::new();
        let mut problems = Vec::new();
        for e in entries {
            // Base and tax per VAT code, in the code's own direction.
            let mut parts: Vec<KhPart> = Vec::new();
            let mut side = None;
            for l in &e.lines {
                let Some(code) = l.vat_code.as_deref() else {
                    continue;
                };
                let (s, sign) = match code.get(..2) {
                    Some("OU") => (KhSide::Supply, -1),
                    Some("IN") => (KhSide::Purchase, 1),
                    Some("RC") => (KhSide::EuService, 1),
                    _ => continue,
                };
                side = Some(s);
                let part = match parts.iter_mut().find(|p| p.vat_code == code) {
                    Some(p) => p,
                    None => {
                        parts.push(KhPart {
                            vat_code: code.to_owned(),
                            base: zero,
                            tax: zero,
                        });
                        parts.last_mut().ok_or(skyla_money::MoneyError::Overflow)?
                    }
                };
                let amount = Money::new(l.functional.minor() * sign, self.currency);
                if l.account == VAT_ACCOUNT {
                    // Reverse charge posts the tax both ways; the statement shows it once.
                    if s == KhSide::EuService && l.functional.minor() > 0 {
                        continue;
                    }
                    let tax = if s == KhSide::EuService {
                        amount.checked_neg()?
                    } else {
                        amount
                    };
                    part.tax = part.tax.checked_add(tax)?;
                } else {
                    part.base = part.base.checked_add(amount)?;
                }
            }
            let Some(side) = side else { continue };
            let (number, counterparty, vat_id, date, corrects_gross) = match side {
                KhSide::Supply => match issued.get(&e.id) {
                    Some(d) => {
                        let corrects = match d.related_id {
                            Some(r) if d.kind == skyla_invoicing::DocKind::CreditNote => Some(
                                skyla_invoicing::get(&self.db(), &self.pack, r)?
                                    .totals
                                    .gross,
                            ),
                            _ => None,
                        };
                        (
                            d.number.clone().unwrap_or_default(),
                            d.customer.name.clone(),
                            d.customer.dic.clone(),
                            d.tax_point_date.clone().unwrap_or_else(|| e.date.clone()),
                            corrects,
                        )
                    }
                    None => {
                        problems.push(format!(
                            "entry #{} has output VAT but no issued document",
                            e.id
                        ));
                        continue;
                    }
                },
                KhSide::Purchase | KhSide::EuService => {
                    let reference = e.source_ref.clone().unwrap_or_default();
                    match self
                        .domain
                        .purchases
                        .iter()
                        .find(|p| p.reference == reference)
                    {
                        Some(p) => (
                            reference,
                            p.supplier.clone(),
                            p.vat_id.clone(),
                            e.date.clone(),
                            None,
                        ),
                        // A receipt with no supplier record: the statement decides
                        // whether that's fine (B.3) or a problem (above the threshold).
                        None => (
                            format!("#{}", e.id),
                            e.memo.clone(),
                            None,
                            e.date.clone(),
                            None,
                        ),
                    }
                }
            };
            docs.push(KhDocument {
                side,
                number,
                counterparty,
                vat_id,
                date,
                parts,
                corrects_gross,
            });
        }
        let kh = control_statement(&self.pack, to, &docs)?;
        problems.extend(kh.problems.iter().cloned());
        // Section C against the return computed from the same ledger.
        let ret = self.vat_return(from, to)?;
        let mut c = Vec::new();
        for (row, base) in &kh.c {
            let in_return = ret
                .rows
                .iter()
                .find(|r| &r.row == row)
                .map_or(0, |r| r.base.minor);
            c.push(KhCRowDto {
                row: row.clone(),
                base: money(*base)?,
                return_base: crate::dto::MoneyDto {
                    minor: in_return,
                    currency: self.currency.code().to_owned(),
                },
                matches: in_return == base.minor(),
            });
        }
        Ok(ControlStatementDto {
            from: from.to_owned(),
            to: to.to_owned(),
            threshold: money(kh.threshold)?,
            matches_return: c.iter().all(|r| r.matches),
            a2: kh.a2.into_iter().map(item_dto).collect::<Result<_, _>>()?,
            a4: kh.a4.into_iter().map(item_dto).collect::<Result<_, _>>()?,
            a5: totals_dto(kh.a5)?,
            b2: kh.b2.into_iter().map(item_dto).collect::<Result<_, _>>()?,
            b3: totals_dto(kh.b3)?,
            c,
            problems,
            due_on: self
                .pack
                .deadline_after("vat.control_statement.due_days_after_period", to)?,
            pack: self.pack.provenance(),
        })
    }
}

impl From<skyla_tax_cz::IncomeError> for CoreError {
    fn from(e: skyla_tax_cz::IncomeError) -> Self {
        match e {
            skyla_tax_cz::IncomeError::Rules(r) => Self::Rules(r),
            skyla_tax_cz::IncomeError::Money(m) => Self::Money(m),
        }
    }
}

fn flat_rate(name: &str) -> Option<FlatRate> {
    match name {
        "craft" => Some(FlatRate::Craft),
        "trade" => Some(FlatRate::Trade),
        "liberal" => Some(FlatRate::Liberal),
        _ => None,
    }
}

fn flat_rate_name(group: FlatRate) -> &'static str {
    match group {
        FlatRate::Craft => "craft",
        FlatRate::Trade => "trade",
        FlatRate::Liberal => "liberal",
    }
}

fn percent_text(rate: skyla_money::Rate) -> String {
    rate.normalize().to_string().replace('.', ",")
}

impl Core {
    /// The § 7 scenarios: from the books so far this year, or from the
    /// user's projection. Every figure is the engine's; the webview only
    /// shows them.
    pub fn income_tax_scenarios(
        &self,
        projection: Option<&TaxProjectionDto>,
    ) -> Result<TaxScenariosDto, CoreError> {
        let as_of = self.domain.entity.as_of.clone();
        let year = as_of.get(..4).unwrap_or("2026").to_owned();
        let on = format!("{year}-12-31");
        let parse = |field: &str, text: &str| {
            skyla_money::parse_amount_cs(text, self.currency).map_err(|_| {
                CoreError::BadRequest(format!(
                    "{field} {text:?} isn't an amount like 1 200 000,00"
                ))
            })
        };
        let (source, from, income, expenses, group, purchase) = match projection {
            None => {
                let cb = cash_basis(&self.db(), &format!("{year}-01-01"), &as_of)?;
                let group = self
                    .domain
                    .entity
                    .flat_rate_group
                    .as_deref()
                    .and_then(flat_rate);
                (
                    "books",
                    format!("{year}-01-01"),
                    cb.taxable_income,
                    cb.deductible_expenses,
                    group,
                    None,
                )
            }
            Some(p) => {
                let income = parse("income", &p.income)?;
                let expenses = parse("expenses", &p.expenses)?;
                if income.is_negative() || expenses.is_negative() {
                    return Err(CoreError::BadRequest(
                        "income and expenses can't be negative".into(),
                    ));
                }
                let group = match p.flat_rate.as_deref() {
                    None | Some("") => None,
                    Some(g) => Some(flat_rate(g).ok_or_else(|| {
                        CoreError::BadRequest(format!("unknown flat-rate group {g:?}"))
                    })?),
                };
                let purchase = if p.purchase_price.trim().is_empty() {
                    None
                } else {
                    let price = parse("purchase price", &p.purchase_price)?;
                    if price.minor() <= 0 {
                        return Err(CoreError::BadRequest(
                            "the purchase price must be above zero".into(),
                        ));
                    }
                    let description = p.purchase_description.trim();
                    Some(PlannedPurchase {
                        description: if description.is_empty() {
                            "The purchase".into()
                        } else {
                            description.to_owned()
                        },
                        price,
                    })
                };
                (
                    "projection",
                    format!("{year}-01-01"),
                    income,
                    expenses,
                    group,
                    purchase,
                )
            }
        };
        let facts = ScenarioFacts {
            section7: Section7 {
                on: on.clone(),
                income,
                actual_expenses: expenses,
            },
            flat_rate: group,
            planned_purchase: purchase.clone(),
        };
        let a = scenarios(&self.pack, &facts)?;
        let mut out = Vec::new();
        for (s, d) in a.scenarios.iter().zip(&a.differences) {
            let w = &s.worksheet;
            let mut label = match w.expenses_method {
                Expenses::Actual => "Actual expenses".to_owned(),
                Expenses::FlatRate(_) => format!(
                    "Flat-rate expenses {} %",
                    w.flat_rate_percent.map(percent_text).unwrap_or_default()
                ),
            };
            if let (Some(p), Some((_, timing))) =
                (&purchase, s.levers.iter().find(|(l, _)| l == "purchase"))
            {
                label.push_str(&format!(
                    " · {} {}",
                    p.description,
                    if timing == "this_year" {
                        "bought this year"
                    } else {
                        "bought next year"
                    }
                ));
            }
            out.push(TaxScenarioDto {
                id: s.id.clone(),
                label,
                expenses: money(w.expenses)?,
                flat_rate_percent: w.flat_rate_percent.map(percent_text),
                flat_rate_cap: w.flat_rate_cap.map(money).transpose()?,
                capped: w.capped,
                profit: money(w.profit)?,
                tax_base: money(w.tax_base)?,
                tax_rate_percent: percent_text(w.tax_rate_percent),
                tax_before_credits: money(w.tax_before_credits)?,
                taxpayer_credit: money(w.taxpayer_credit)?,
                tax: money(w.tax)?,
                social_base: money(w.social.assessment_base)?,
                social_rate_percent: percent_text(w.social.rate_percent),
                social: money(w.social.amount)?,
                health_base: money(w.health.assessment_base)?,
                health_rate_percent: percent_text(w.health.rate_percent),
                health: money(w.health.amount)?,
                total: money(w.total)?,
                vs_baseline_tax: money(d.tax)?,
                vs_baseline_insurance: money(d.social.checked_add(d.health)?)?,
                vs_baseline_total: money(d.total)?,
                vs_baseline_pension_base: money(d.pension_base)?,
            });
        }
        let mut assumptions = a
            .scenarios
            .first()
            .map(|s| s.worksheet.assumptions.clone())
            .unwrap_or_default();
        if source == "books" {
            assumptions.insert(
                0,
                format!(
                    "From the books, {from} to {as_of} (cash basis), not a projection for the whole year."
                ),
            );
        }
        Ok(TaxScenariosDto {
            year,
            source: source.to_owned(),
            from,
            to: if source == "books" { as_of } else { on },
            income: money(income)?,
            actual_expenses: money(expenses)?,
            flat_rate: group.map(|g| flat_rate_name(g).to_owned()),
            baseline: a.baseline,
            lowest_total: a.lowest_total,
            scenarios: out,
            questions: a.questions,
            not_evaluated: a.not_evaluated,
            assumptions,
            pack: self.pack.provenance(),
        })
    }
}

impl Core {
    /// The entity facts the pack's obligations are keyed by.
    fn obligation_facts(&self) -> Vec<&'static str> {
        let e = &self.domain.entity;
        let mut facts = Vec::new();
        if e.legal_form == "OSVČ" {
            facts.push("osvc");
        }
        match e.vat_period.as_str() {
            "monthly" => facts.push("vat_monthly"),
            "quarterly" => facts.push("vat_quarterly"),
            _ => {}
        }
        facts
    }

    /// Every deadline of `year` from the pack, with where each comes from.
    pub fn obligations(&self, year: i32) -> Result<Vec<ObligationDto>, CoreError> {
        let as_of = &self.domain.entity.as_of;
        let deadlines = self
            .pack
            .calendar(i64::from(year), &self.obligation_facts())?;
        let next = deadlines
            .iter()
            .map(|d| d.due.as_str())
            .find(|due| *due >= as_of.as_str())
            .map(str::to_owned);
        Ok(deadlines
            .into_iter()
            .map(|d| {
                let act = self
                    .pack
                    .acts
                    .get(&d.cite.act)
                    .map_or(d.cite.act.clone(), |a| a.name.clone());
                let status = if d.due.as_str() < as_of.as_str() {
                    "past"
                } else if Some(&d.due) == next.as_ref() {
                    "next"
                } else {
                    "upcoming"
                };
                ObligationDto {
                    shifted: d.nominal != d.due,
                    obligation: d.obligation,
                    name: d.name,
                    action: d.action,
                    period: d.period,
                    nominal: d.nominal,
                    due: d.due,
                    citation: format!("{act}, {}", d.cite.section),
                    status: status.to_owned(),
                }
            })
            .collect())
    }
}
