//! Kontrolní hlášení (DPH control statement, § 101c–101i ZDPH).
//!
//! Every document with VAT in the period lands in one section:
//! supplies to VAT payers above the pack's threshold (VAT included) are
//! itemised in A.4, everything else sold goes into A.5 as totals;
//! purchases from VAT payers above it are itemised in B.2, the rest in
//! B.3; services received from the EU under reverse charge are itemised
//! in A.2. A correction follows its original's section. Section C totals
//! the bases by return row, so it can be checked against the DPH return.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use skyla_money::{Currency, Money};
use skyla_rules::Pack;

/// Which way a document goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KhSide {
    /// A supply we made.
    Supply,
    /// A purchase from a domestic VAT payer.
    Purchase,
    /// A service received from another member state (reverse charge).
    EuService,
}

/// A document's VAT under one code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KhPart {
    /// The pack's VAT code.
    pub vat_code: String,
    /// Base (positive for the normal direction, negative for a correction).
    pub base: Money,
    /// Tax.
    pub tax: Money,
}

/// A document with VAT in the period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KhDocument {
    /// Which way.
    pub side: KhSide,
    /// The document's evidence number.
    pub number: String,
    /// The other party.
    pub counterparty: String,
    /// The other party's VAT id; none for someone not registered.
    pub vat_id: Option<String>,
    /// DUZP for supplies, DPPD for purchases.
    pub date: String,
    /// Its VAT, by code.
    pub parts: Vec<KhPart>,
    /// For a correction: the original document's gross (VAT included),
    /// which decides the section.
    pub corrects_gross: Option<Money>,
}

/// An itemised line (A.2, A.4, B.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KhItem {
    /// The document.
    pub number: String,
    /// The other party.
    pub counterparty: String,
    /// Their VAT id.
    pub vat_id: String,
    /// DUZP or DPPD.
    pub date: String,
    /// Base and tax at the standard rate.
    pub base_standard: Money,
    /// Tax at the standard rate.
    pub tax_standard: Money,
    /// Base at the reduced rate.
    pub base_reduced: Money,
    /// Tax at the reduced rate.
    pub tax_reduced: Money,
}

/// Totals for the summary sections (A.5, B.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KhTotals {
    /// Base at the standard rate.
    pub base_standard: Money,
    /// Tax at the standard rate.
    pub tax_standard: Money,
    /// Base at the reduced rate.
    pub base_reduced: Money,
    /// Tax at the reduced rate.
    pub tax_reduced: Money,
    /// Documents counted.
    pub documents: u32,
}

/// The control statement for a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlStatement {
    /// The threshold applied (VAT included), from the pack.
    pub threshold: Money,
    /// Services received from the EU.
    pub a2: Vec<KhItem>,
    /// Supplies to VAT payers above the threshold.
    pub a4: Vec<KhItem>,
    /// Every other supply.
    pub a5: KhTotals,
    /// Purchases from VAT payers above the threshold.
    pub b2: Vec<KhItem>,
    /// Every other purchase.
    pub b3: KhTotals,
    /// Section C: bases by DPH return row.
    pub c: BTreeMap<String, Money>,
    /// Documents that couldn't be placed, and why.
    pub problems: Vec<String>,
}

/// Errors from the pack.
pub type KhError = skyla_rules::RulesError;

enum Band {
    Standard,
    Reduced,
}

fn band(pack: &Pack, vat_code: &str) -> Result<Option<Band>, KhError> {
    let code = pack.vat_code(vat_code)?;
    Ok(match code.rate.as_str() {
        "vat.rate.standard" => Some(Band::Standard),
        "vat.rate.reduced" => Some(Band::Reduced),
        _ => None,
    })
}

/// The DPH return rows section C repeats (DPHKH1 section C: ř. 1, 2, 10,
/// 11, 25, 40, 41). Part of the form's structure, not a statutory value.
const SECTION_C_ROWS: [&str; 7] = ["1", "2", "10", "11", "25", "40", "41"];

fn add(a: Money, b: Money) -> Money {
    a.checked_add(b).unwrap_or(a)
}

/// Sorts a period's documents into the control statement's sections.
pub fn control_statement(
    pack: &Pack,
    period_end: &str,
    documents: &[KhDocument],
) -> Result<ControlStatement, KhError> {
    let threshold = pack.amount("vat.control_statement.itemise_above", period_end)?;
    let currency: Currency = threshold.currency();
    let zero = Money::zero(currency);
    let totals = || KhTotals {
        base_standard: zero,
        tax_standard: zero,
        base_reduced: zero,
        tax_reduced: zero,
        documents: 0,
    };
    let mut out = ControlStatement {
        threshold,
        a2: Vec::new(),
        a4: Vec::new(),
        a5: totals(),
        b2: Vec::new(),
        b3: totals(),
        c: BTreeMap::new(),
        problems: Vec::new(),
    };
    for d in documents {
        let (mut bs, mut ts, mut br, mut tr) = (zero, zero, zero, zero);
        let mut gross = zero;
        for p in &d.parts {
            match band(pack, &p.vat_code)? {
                Some(Band::Standard) => {
                    bs = add(bs, p.base);
                    ts = add(ts, p.tax);
                }
                Some(Band::Reduced) => {
                    br = add(br, p.base);
                    tr = add(tr, p.tax);
                }
                None => continue, // outside VAT: not in the statement
            }
            gross = add(gross, add(p.base, p.tax));
            // Section C: the base on the code's return row.
            let code = pack.vat_code(&p.vat_code)?;
            for row in code.rows.iter().filter(|r| {
                r.part == skyla_rules::RowPart::Base && SECTION_C_ROWS.contains(&r.row.as_str())
            }) {
                let entry = out.c.entry(row.row.clone()).or_insert(zero);
                *entry = add(*entry, p.base);
            }
        }
        if bs.is_zero() && br.is_zero() && ts.is_zero() && tr.is_zero() {
            continue;
        }
        let decisive = d.corrects_gross.unwrap_or(gross);
        let above = decisive.minor().abs() > threshold.minor();
        let item = |vat_id: &str| KhItem {
            number: d.number.clone(),
            counterparty: d.counterparty.clone(),
            vat_id: vat_id.to_owned(),
            date: d.date.clone(),
            base_standard: bs,
            tax_standard: ts,
            base_reduced: br,
            tax_reduced: tr,
        };
        let sum_into = |t: &mut KhTotals| {
            t.base_standard = add(t.base_standard, bs);
            t.tax_standard = add(t.tax_standard, ts);
            t.base_reduced = add(t.base_reduced, br);
            t.tax_reduced = add(t.tax_reduced, tr);
            t.documents += 1;
        };
        match (d.side, d.vat_id.as_deref()) {
            (KhSide::EuService, Some(id)) => out.a2.push(item(id)),
            (KhSide::EuService, None) => out.problems.push(format!(
                "{}: a service from the EU needs the supplier's VAT id for A.2",
                d.number
            )),
            (KhSide::Supply, Some(id)) if above => out.a4.push(item(id)),
            (KhSide::Supply, _) => sum_into(&mut out.a5),
            (KhSide::Purchase, Some(id)) if above => out.b2.push(item(id)),
            (KhSide::Purchase, None) if above => out.problems.push(format!(
                "{}: a purchase above the threshold needs the supplier's DIČ for B.2",
                d.number
            )),
            // A simplified receipt names no buyer; B.3 needs no DIČ.
            (KhSide::Purchase, _) => sum_into(&mut out.b3),
        }
    }
    let key = |i: &KhItem| (i.date.clone(), i.number.clone());
    out.a2.sort_by_key(key);
    out.a4.sort_by_key(key);
    out.b2.sort_by_key(key);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn czk(m: i64) -> Money {
        Money::new(m, Currency::CZK)
    }

    fn doc(
        side: KhSide,
        number: &str,
        vat_id: Option<&str>,
        parts: &[(&str, i64, i64)],
    ) -> KhDocument {
        KhDocument {
            side,
            number: number.into(),
            counterparty: "X".into(),
            vat_id: vat_id.map(Into::into),
            date: "2026-09-15".into(),
            parts: parts
                .iter()
                .map(|(c, b, t)| KhPart {
                    vat_code: (*c).into(),
                    base: czk(*b),
                    tax: czk(*t),
                })
                .collect(),
            corrects_gross: None,
        }
    }

    #[test]
    fn the_threshold_splits_itemised_from_summed() {
        let pack = Pack::cz_2026().expect("pack");
        let payer = Some("CZ91341272");
        let mut credit = doc(
            KhSide::Supply,
            "OD260001",
            payer,
            &[("OUT21", -100_000, -21_000)],
        );
        credit.corrects_gross = Some(czk(1_210_000));
        let docs = [
            // 8 264,46 + 1 735,54 = 10 000,00 exactly: not above, so summed.
            doc(
                KhSide::Supply,
                "2026-001",
                payer,
                &[("OUT21", 826_446, 173_554)],
            ),
            // One haléř more is itemised.
            doc(
                KhSide::Supply,
                "2026-002",
                payer,
                &[("OUT21", 826_447, 173_554)],
            ),
            // A big sale to someone not registered is summed.
            doc(
                KhSide::Supply,
                "2026-003",
                None,
                &[("OUT21", 5_000_000, 1_050_000)],
            ),
            // A small correction of an itemised invoice stays itemised.
            credit,
            doc(
                KhSide::Purchase,
                "PF-1",
                Some("CZ92588034"),
                &[("IN21", 1_000_000, 210_000), ("IN12", 200_000, 24_000)],
            ),
            doc(
                KhSide::Purchase,
                "PF-2",
                Some("CZ92588034"),
                &[("IN21", 500_000, 105_000)],
            ),
            doc(
                KhSide::EuService,
                "AWS-9",
                Some("LU99999999"),
                &[("RC21S", 1_231_860, 258_691)],
            ),
            // A small receipt without the supplier's DIČ is summed; a big one can't be placed.
            doc(KhSide::Purchase, "receipt", None, &[("IN21", 100, 21)]),
            doc(KhSide::Purchase, "?", None, &[("IN21", 900_000, 189_000)]),
        ];
        let kh = control_statement(&pack, "2026-09-30", &docs).expect("kh");
        assert_eq!(kh.threshold, czk(1_000_000));
        assert_eq!(
            kh.a4.iter().map(|i| i.number.as_str()).collect::<Vec<_>>(),
            ["2026-002", "OD260001"]
        );
        assert_eq!(
            (kh.a5.documents, kh.a5.base_standard),
            (2, czk(826_446 + 5_000_000))
        );
        assert_eq!(kh.b2.len(), 1);
        assert_eq!(
            (
                kh.b2[0].base_standard,
                kh.b2[0].base_reduced,
                kh.b2[0].tax_reduced
            ),
            (czk(1_000_000), czk(200_000), czk(24_000))
        );
        assert_eq!(
            (kh.b3.documents, kh.b3.base_standard),
            (2, czk(500_000 + 100))
        );
        assert_eq!(kh.a2.len(), 1);
        assert_eq!(
            kh.problems,
            ["?: a purchase above the threshold needs the supplier's DIČ for B.2"]
        );
        // C: ř. 1 holds every supply's base, ř. 40 and 41 the purchases'; A.2 isn't in C.
        assert_eq!(
            kh.c.get("1"),
            Some(&czk(826_446 + 826_447 + 5_000_000 - 100_000))
        );
        assert_eq!(kh.c.get("40"), Some(&czk(1_500_000 + 100 + 900_000)));
        assert_eq!(kh.c.get("41"), Some(&czk(200_000)));
        assert!(!kh.c.contains_key("5") && !kh.c.contains_key("43"));
    }
}
