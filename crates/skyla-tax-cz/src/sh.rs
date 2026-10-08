//! Souhrnné hlášení (EC Sales List, § 102 ZDPH).
//!
//! A VAT payer reports its supplies to VAT payers in other member states
//! (goods under § 64, services whose place of supply is the customer's
//! state) per customer VAT number and supply code. Which VAT codes count,
//! and the statement's code for each, come from the rule pack (`eu_supply`
//! on the VAT code); nothing here names a code. A correction follows the
//! document it corrects into that document's period, so the statement for
//! the period shows what the books now say about it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use skyla_money::{Currency, Money, MoneyError};
use skyla_rules::{Pack, RulesError};

/// Errors from the statement.
#[derive(Debug, thiserror::Error)]
pub enum ShError {
    /// A VAT code isn't in the pack.
    #[error(transparent)]
    Rules(#[from] RulesError),
    /// Arithmetic left the representable range.
    #[error(transparent)]
    Money(#[from] MoneyError),
}

/// A document's base under one VAT code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShPart {
    /// The pack's VAT code.
    pub vat_code: String,
    /// The base (negative on a correction).
    pub base: Money,
}

/// An issued document, as the statement needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShDocument {
    /// The document's number.
    pub number: String,
    /// The customer's name.
    pub counterparty: String,
    /// The customer's VAT number, as typed.
    pub vat_id: Option<String>,
    /// The date of the taxable supply.
    pub date: String,
    /// For a correction: the date of the supply it corrects, which decides
    /// the period.
    pub corrects_date: Option<String>,
    /// Its bases, by VAT code.
    pub parts: Vec<ShPart>,
}

/// One line of the statement: a customer and a supply code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShLine {
    /// The customer's country prefix (`DE`, `EL`).
    pub country: String,
    /// The customer's number without the prefix.
    pub vat_number: String,
    /// The supply code (kód plnění).
    pub sh_code: String,
    /// The customer's name, from their first document.
    pub counterparty: String,
    /// The total base of the supplies, corrections included.
    pub base: Money,
    /// How many supplies (documents that aren't corrections).
    pub supplies: u32,
}

/// The statement for a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecapitulativeStatement {
    /// First day.
    pub from: String,
    /// Last day.
    pub to: String,
    /// Per customer and supply code, ordered by country, number and code.
    /// A customer whose supplies net to nothing (fully corrected) is left out.
    pub lines: Vec<ShLine>,
    /// Sum of the lines' bases.
    pub total: Money,
    /// Documents that couldn't be placed, and why.
    pub problems: Vec<String>,
}

/// A VAT number split into its country prefix and national part, or `None`
/// when it doesn't look like one of another member state. The Czech prefix
/// is refused: a supply to a Czech customer is domestic.
fn split_vat_id(text: &str) -> Option<(String, String)> {
    let compact: String = text
        .chars()
        .filter(|c| !matches!(c, ' ' | '.' | '-' | '\u{a0}'))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let (country, number) = (compact.get(..2)?, compact.get(2..)?);
    let ok = country.bytes().all(|b| b.is_ascii_uppercase())
        && country != "CZ"
        && !number.is_empty()
        && number.len() <= 12
        && number.bytes().all(|b| b.is_ascii_alphanumeric());
    ok.then(|| (country.to_owned(), number.to_owned()))
}

/// Sorts the EU supplies of `from..=to` into the souhrnné hlášení.
///
/// A document counts in the period of its supply (`date`), or for a
/// correction, of the supply it corrects (`corrects_date`).
pub fn recapitulative_statement(
    pack: &Pack,
    from: &str,
    to: &str,
    currency: Currency,
    documents: &[ShDocument],
) -> Result<RecapitulativeStatement, ShError> {
    let zero = Money::zero(currency);
    let mut groups: BTreeMap<(String, String, String), ShLine> = BTreeMap::new();
    let mut problems = Vec::new();
    for d in documents {
        let period_date = d.corrects_date.as_deref().unwrap_or(&d.date);
        if period_date < from || period_date > to {
            continue;
        }
        // The bases that are EU supplies, per the pack's statement code.
        let mut eu: Vec<(&str, Money)> = Vec::new();
        for p in &d.parts {
            if let Some(supply) = &pack.vat_code(&p.vat_code)?.eu_supply {
                eu.push((supply.sh_code.as_str(), p.base));
            }
        }
        if eu.is_empty() {
            continue;
        }
        let Some((country, number)) = d.vat_id.as_deref().and_then(split_vat_id) else {
            problems.push(format!(
                "{}: the souhrnné hlášení needs the customer's VAT number in another member state",
                d.number
            ));
            continue;
        };
        for (code, base) in eu {
            let line = groups
                .entry((country.clone(), number.clone(), code.to_owned()))
                .or_insert_with(|| ShLine {
                    country: country.clone(),
                    vat_number: number.clone(),
                    sh_code: code.to_owned(),
                    counterparty: d.counterparty.clone(),
                    base: zero,
                    supplies: 0,
                });
            line.base = line.base.checked_add(base)?;
            if d.corrects_date.is_none() {
                line.supplies += 1;
            }
        }
    }
    let lines: Vec<ShLine> = groups.into_values().filter(|l| !l.base.is_zero()).collect();
    let total = Money::sum(currency, lines.iter().map(|l| l.base))?;
    Ok(RecapitulativeStatement {
        from: from.to_owned(),
        to: to.to_owned(),
        lines,
        total,
        problems,
    })
}
