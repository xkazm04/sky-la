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
    /// A supply to a VAT payer in another member state: no Czech VAT.
    pub(crate) eu_supply: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedRecap {
    pub(crate) rate: Rate,
    pub(crate) outside_vat: bool,
    pub(crate) eu_supply: bool,
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
            eu_supply: code.eu_supply.is_some(),
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
            eu_supply: code.eu_supply.is_some(),
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

/// A document in EN 16931 terms: positive amounts on credit notes, taxed
/// advances deducted as negative lines, the VAT breakdown per category and
/// rate, and the document totals (BG-22).
#[derive(Debug, Clone)]
pub(crate) struct En16931 {
    /// UNTDID 1001: 380 invoice, 381 credit note, 386 prepayment invoice.
    pub(crate) type_code: &'static str,
    pub(crate) lines: Vec<EnLine>,
    pub(crate) taxes: Vec<EnTax>,
    pub(crate) line_total: Money,
    pub(crate) tax_total: Money,
    pub(crate) tax_inclusive: Money,
    pub(crate) prepaid: Money,
    pub(crate) payable: Money,
    pub(crate) vat_payer: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct EnLine {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) quantity: String,
    pub(crate) unit_code: &'static str,
    pub(crate) net: Money,
    pub(crate) price: Money,
    pub(crate) category: String,
    pub(crate) rate: Option<Rate>,
}

#[derive(Debug, Clone)]
pub(crate) struct EnTax {
    pub(crate) category: String,
    pub(crate) rate: Option<Rate>,
    pub(crate) taxable: Money,
    pub(crate) tax: Money,
    pub(crate) exemption_reason: Option<String>,
}

pub(crate) fn en16931(pack: &Pack, input: ExportInput<'_>) -> Result<En16931, InvoicingError> {
    let p = prepare(pack, input)?;
    let doc = input.doc;
    let type_code = match doc.kind {
        DocKind::Invoice => "380",
        DocKind::CreditNote => "381",
        DocKind::AdvanceTax => "386",
        DocKind::Advance => {
            return Err(InvoicingError::Invalid(vec![
                "an advance invoice isn't a tax document; EN 16931 covers its tax document instead"
                    .into(),
            ]));
        }
    };
    let currency = p.gross.currency();
    let zero = Money::zero(currency);
    // Credit notes state positive amounts; the type code carries the sign.
    let neg = doc.kind == DocKind::CreditNote;
    let signed = |m: Money| if neg { m.checked_neg() } else { Ok(m) };
    let on = doc
        .tax_point_date
        .clone()
        .or_else(|| doc.issue_date.clone())
        .unwrap_or_default();
    let category = |vat_code: &str| -> Result<skyla_rules::EinvoiceTax, InvoicingError> {
        pack.vat_code(vat_code)?.einvoice.clone().ok_or_else(|| {
            InvoicingError::Invalid(vec![format!(
                "VAT code {vat_code} has no EN 16931 category in the rule pack"
            )])
        })
    };
    let rate_of = |cat: &str, rate: Rate| (cat != "O" && cat != "E").then_some(rate);

    let mut lines = Vec::new();
    for (l, src) in p.lines.iter().zip(&doc.lines) {
        let cat = category(&src.input.vat_code)?;
        let quantity = if neg {
            src.input
                .quantity
                .strip_prefix('-')
                .map_or_else(|| format!("-{}", src.input.quantity), str::to_owned)
        } else {
            src.input.quantity.clone()
        };
        lines.push(EnLine {
            id: l.no.to_string(),
            name: l.description.clone(),
            quantity,
            unit_code: un_ece_unit(&l.unit),
            net: signed(l.base)?,
            price: l.unit_price,
            rate: rate_of(&cat.category, l.rate),
            category: cat.category,
        });
    }
    // Advances already taxed come off as negative lines, one per document and rate.
    let mut next = lines.len() + 1;
    for adv in input.advances {
        let number = adv.number.as_deref().unwrap_or_default();
        let adv_on = adv.tax_point_date.as_deref().unwrap_or(&on);
        for r in &adv.totals.recap {
            let cat = category(&r.vat_code)?;
            let rate = pack.vat_rate(&r.vat_code, adv_on)?;
            lines.push(EnLine {
                id: next.to_string(),
                name: format!("Odpočet zálohy {number}"),
                quantity: "-1".into(),
                unit_code: "C62",
                net: r.base.checked_neg()?,
                price: r.base,
                rate: rate_of(&cat.category, rate),
                category: cat.category,
            });
            next += 1;
        }
    }

    let mut taxes = Vec::new();
    for (r, src) in p.recap.iter().zip(&doc.totals.recap) {
        let cat = category(&src.vat_code)?;
        taxes.push(EnTax {
            rate: rate_of(&cat.category, r.rate),
            category: cat.category,
            taxable: signed(r.base.checked_sub(r.claimed_base)?)?,
            tax: signed(r.vat.checked_sub(r.claimed_vat)?)?,
            exemption_reason: cat.exemption_reason,
        });
    }
    let line_total = Money::sum(currency, lines.iter().map(|l| l.net))?;
    let tax_total = Money::sum(currency, taxes.iter().map(|t| t.tax))?;
    let tax_inclusive = line_total.checked_add(tax_total)?;
    // A tax document on a received advance records a payment already made.
    let prepaid = if doc.kind == DocKind::AdvanceTax {
        tax_inclusive
    } else {
        zero
    };
    Ok(En16931 {
        type_code,
        lines,
        taxes,
        line_total,
        tax_total,
        tax_inclusive,
        prepaid,
        payable: tax_inclusive.checked_sub(prepaid)?,
        vat_payer: p.vat_payer,
    })
}

/// UN/ECE Recommendation 20 unit codes; `C62` (one) for anything else.
pub(crate) fn un_ece_unit(unit: &str) -> &'static str {
    match unit.trim() {
        "h" | "hod" | "hodina" => "HUR",
        "ks" | "pc" | "pcs" | "kus" => "H87",
        "den" | "day" | "d" => "DAY",
        "měs" | "month" => "MON",
        "km" => "KMT",
        "kg" => "KGM",
        _ => "C62",
    }
}

/// The Peppol electronic address scheme (EAS) for a VAT number, by its
/// country prefix.
pub(crate) fn vat_endpoint_scheme(vat_id: &str) -> Option<&'static str> {
    match vat_id.get(..2)? {
        "CZ" => Some("9929"),
        "SK" => Some("9950"),
        "DE" => Some("9930"),
        "AT" => Some("9914"),
        "PL" => Some("9945"),
        "HU" => Some("9910"),
        _ => None,
    }
}

/// `20260825` for `2026-08-25`.
pub(crate) fn compact_date(date: &str) -> String {
    date.replace('-', "")
}
