//! The kontrolní hlášení from the books (WP-21): every posted entry with
//! VAT in the period becomes a document, with the counterparty from the
//! invoicing module (supplies) or the purchase records (received invoices),
//! and section C is checked against the DPH return for the same period.

use std::collections::HashMap;

use skyla_ledger::list_posted;
use skyla_money::Money;
use skyla_tax_cz::{KhDocument, KhItem, KhPart, KhSide, KhTotals, control_statement};

use super::{Core, money};
use crate::dto::{ControlStatementDto, KhCRowDto, KhItemDto, KhTotalsDto};
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
