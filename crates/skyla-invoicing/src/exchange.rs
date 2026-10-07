//! What every e-invoice format needs from a document, computed once in Rust:
//! per-line VAT, tax-inclusive unit prices, the recapitulation with the
//! advances already taxed, and the amount payable. The writers only format.

use skyla_money::{Money, Rate, vat};
use skyla_rules::Pack;

use crate::{DocKind, Document, InvoicingError};

/// A document ready for export, with the documents it refers to.
#[derive(Debug, Clone, Copy)]
pub struct ExportInput<'a> {
    /// The document; it must be issued.
    pub doc: &'a Document,
    /// The invoice a credit note corrects, or the advance a tax document covers.
    pub related: Option<&'a Document>,
    /// The advance tax documents a final invoice deducts.
    pub advances: &'a [Document],
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedLine {
    pub(crate) no: i64,
    pub(crate) description: String,
    pub(crate) quantity: String,
    pub(crate) unit: String,
    pub(crate) unit_price: Money,
    pub(crate) unit_price_gross: Money,
    pub(crate) base: Money,
    pub(crate) vat: Money,
    pub(crate) gross: Money,
    pub(crate) rate: Rate,
    pub(crate) outside_vat: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedRecap {
    pub(crate) rate: Rate,
    pub(crate) outside_vat: bool,
    pub(crate) base: Money,
    pub(crate) vat: Money,
    pub(crate) claimed_base: Money,
    pub(crate) claimed_vat: Money,
}

#[derive(Debug, Clone)]
pub(crate) struct Prepared {
    pub(crate) lines: Vec<PreparedLine>,
    pub(crate) recap: Vec<PreparedRecap>,
    pub(crate) base: Money,
    pub(crate) vat: Money,
    pub(crate) gross: Money,
    pub(crate) claimed_base: Money,
    pub(crate) claimed_gross: Money,
    pub(crate) payable: Money,
    pub(crate) vat_payer: bool,
}

pub(crate) fn prepare(pack: &Pack, input: ExportInput<'_>) -> Result<Prepared, InvoicingError> {
    let doc = input.doc;
    let mut problems = Vec::new();
    if !doc.issued {
        problems.push("only issued documents are exported".to_owned());
    }
    if doc.supplier.is_none() {
        problems.push("the document has no supplier profile".to_owned());
    }
    if !problems.is_empty() {
        return Err(InvoicingError::Invalid(problems));
    }
    let on = doc
        .tax_point_date
        .clone()
        .or_else(|| doc.issue_date.clone())
        .unwrap_or_default();
    let rounding = pack.rounding("vat.rounding.document", &on)?;
    let currency = doc.totals.gross.currency();
    let zero = Money::zero(currency);
    let split = |amount: Money, rate: Rate| {
        if doc.kind == DocKind::AdvanceTax {
            vat::from_gross(amount, rate, rounding)
        } else {
            vat::from_base(amount, rate, rounding)
        }
    };

    let mut lines = Vec::new();
    for l in &doc.lines {
        let code = pack.vat_code(&l.input.vat_code)?;
        let rate = pack.vat_rate(&l.input.vat_code, &on)?;
        let amount = split(l.amount, rate)?;
        let unit = split(Money::new(l.input.unit_price_minor, currency), rate)?;
        lines.push(PreparedLine {
            no: l.line_no,
            description: l.input.description.clone(),
            quantity: l.input.quantity.clone(),
            unit: l.input.unit.clone(),
            unit_price: unit.base,
            unit_price_gross: unit.base.checked_add(unit.vat)?,
            base: amount.base,
            vat: amount.vat,
            gross: amount.base.checked_add(amount.vat)?,
            rate,
            outside_vat: code.outside_vat,
        });
    }

    let mut recap = Vec::new();
    for r in &doc.totals.recap {
        let code = pack.vat_code(&r.vat_code)?;
        let (mut claimed_base, mut claimed_vat) = (zero, zero);
        for adv in input.advances {
            for a in adv.totals.recap.iter().filter(|a| a.vat_code == r.vat_code) {
                claimed_base = claimed_base.checked_add(a.base)?;
                claimed_vat = claimed_vat.checked_add(a.vat)?;
            }
        }
        recap.push(PreparedRecap {
            rate: pack.vat_rate(&r.vat_code, &on)?,
            outside_vat: code.outside_vat,
            base: r.base,
            vat: r.vat,
            claimed_base,
            claimed_vat,
        });
    }
    let claimed_base = Money::sum(currency, recap.iter().map(|r| r.claimed_base))?;
    let claimed_vat = Money::sum(currency, recap.iter().map(|r| r.claimed_vat))?;
    let claimed_gross = claimed_base.checked_add(claimed_vat)?;
    Ok(Prepared {
        lines,
        recap,
        base: doc.totals.base,
        vat: doc.totals.vat,
        gross: doc.totals.gross,
        claimed_base,
        claimed_gross,
        payable: doc.totals.gross.checked_sub(claimed_gross)?,
        vat_payer: doc.supplier.as_ref().is_some_and(|s| s.vat_payer),
    })
}

/// A Czech IBAN's domestic form: (`prefix-number` or `number`, bank code).
pub(crate) fn czech_account(iban: &str) -> Option<(String, String)> {
    if !iban.starts_with("CZ") || iban.len() != 24 {
        return None;
    }
    let bank = iban[4..8].to_owned();
    let prefix = iban[8..14].trim_start_matches('0');
    let number = iban[14..24].trim_start_matches('0');
    let account = if prefix.is_empty() {
        number.to_owned()
    } else {
        format!("{prefix}-{number}")
    };
    Some((account, bank))
}
