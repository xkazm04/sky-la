//! The template's input: every printed string, prepared in Rust.

use serde::Serialize;
use skyla_invoicing::spayd::{document_payment, spayd, variable_symbol};
use skyla_invoicing::{DocKind, Document};
use skyla_money::{Currency, Money, format_amount_cs};

use crate::RenderError;

/// The document's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    /// Czech.
    Cs,
    /// English.
    En,
}

/// What the template prints, all of it formatted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InvoiceView {
    /// Labels and formats follow this language.
    pub lang: Lang,
    /// `invoice`, `credit_note`, `advance` or `advance_tax`.
    pub kind: &'static str,
    /// Whether the supplier charges VAT (changes the title and columns).
    pub vat_payer: bool,
    /// False for drafts: the template prints a draft mark.
    pub issued: bool,
    /// Document number (or the draft's placeholder).
    pub number: String,
    /// The invoice a credit note corrects or the advance a tax document covers.
    pub related_number: Option<String>,
    /// Formatted dates.
    pub issue_date: Option<String>,
    /// Date of the taxable supply.
    pub tax_point_date: Option<String>,
    /// Due date.
    pub due_date: Option<String>,
    /// The issuing business.
    pub supplier: Party,
    /// The customer.
    pub customer: Party,
    /// Lines.
    pub lines: Vec<LineView>,
    /// VAT recapitulation, one row per rate.
    pub recap: Vec<RecapView>,
    /// Sum of bases.
    pub base: String,
    /// Sum of VAT.
    pub vat: String,
    /// Base plus VAT.
    pub gross: String,
    /// Advances already paid and deducted, if any.
    pub advances: Option<String>,
    /// What's left to pay, with the currency.
    pub due: String,
    /// `Kč`, `€` or the ISO code.
    pub currency: String,
    /// Payment details.
    pub payment: Option<PaymentView>,
    /// The SPAYD payload of the QR code.
    pub spayd: Option<String>,
    /// Reverse-charge wording is needed.
    pub reverse_charge: bool,
    /// Free text from the document.
    pub note: String,
    /// `cz-2026@2026.1`: the rule pack the totals came from.
    pub pack: Option<String>,
}

/// A supplier or customer block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Party {
    /// Name.
    pub name: String,
    /// Address lines.
    pub address: Vec<String>,
    /// IČO.
    pub ico: Option<String>,
    /// DIČ.
    pub dic: Option<String>,
    /// Registration line (supplier only).
    pub registration: Option<String>,
    /// E-mail (supplier only).
    pub email: Option<String>,
}

/// One printed line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineView {
    /// What was supplied.
    pub description: String,
    /// Quantity with its unit.
    pub quantity: String,
    /// Price per unit, without VAT.
    pub unit_price: String,
    /// VAT rate, e.g. `21 %`.
    pub rate: String,
    /// Line amount, without VAT.
    pub amount: String,
}

/// One VAT recap row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecapView {
    /// Rate, e.g. `21 %`.
    pub rate: String,
    /// Base.
    pub base: String,
    /// VAT.
    pub vat: String,
    /// Base plus VAT.
    pub gross: String,
}

/// Bank details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PaymentView {
    /// IBAN in groups of four.
    pub iban: String,
    /// BIC.
    pub bic: Option<String>,
    /// Variable symbol.
    pub variable_symbol: Option<String>,
}

/// Prepares a document for the template. `due` is what's left to pay after
/// deducted advances; `related` is the number of the invoice a credit note
/// corrects, or of the advance a tax document covers.
pub fn invoice_view(
    doc: &Document,
    due: Money,
    related: Option<&str>,
    lang: Lang,
) -> Result<InvoiceView, RenderError> {
    let currency = due.currency();
    let fmt = |m: Money| amount(m.minor(), currency, lang);
    let supplier = doc.supplier.clone();
    let vat_payer = supplier.as_ref().is_none_or(|s| s.vat_payer);
    let rate_of = |code: &str| {
        doc.totals
            .recap
            .iter()
            .find(|r| r.vat_code == code)
            .map(|r| percent(&r.rate, lang))
            .unwrap_or_default()
    };
    let lines = doc
        .lines
        .iter()
        .map(|l| LineView {
            description: l.input.description.clone(),
            quantity: format!("{} {}", decimal_text(&l.input.quantity, lang), l.input.unit)
                .trim_end()
                .to_owned(),
            unit_price: amount(l.input.unit_price_minor, currency, lang),
            rate: rate_of(&l.input.vat_code),
            amount: fmt(l.amount),
        })
        .collect();
    let recap = doc
        .totals
        .recap
        .iter()
        .map(|r| {
            Ok(RecapView {
                rate: percent(&r.rate, lang),
                base: fmt(r.base),
                vat: fmt(r.vat),
                gross: fmt(r.base.checked_add(r.vat)?),
            })
        })
        .collect::<Result<Vec<_>, skyla_money::MoneyError>>()
        .map_err(skyla_invoicing::InvoicingError::from)?;
    let deducted = doc.totals.gross.minor() - due.minor();
    let payment_request = document_payment(doc, due);
    let spayd = payment_request.as_ref().map(spayd).transpose()?;
    let payment = supplier.as_ref().and_then(|s| {
        s.iban.as_ref().map(|iban| PaymentView {
            iban: grouped(iban),
            bic: s.bic.clone(),
            variable_symbol: doc.number.as_deref().and_then(variable_symbol),
        })
    });
    let symbol = match currency.code() {
        "CZK" => "Kč".to_owned(),
        "EUR" => "€".to_owned(),
        other => other.to_owned(),
    };
    Ok(InvoiceView {
        lang,
        kind: doc.kind.as_str(),
        vat_payer,
        issued: doc.issued,
        number: doc.number.clone().unwrap_or_else(|| match lang {
            Lang::Cs => "koncept".to_owned(),
            Lang::En => "draft".to_owned(),
        }),
        related_number: related
            .filter(|_| matches!(doc.kind, DocKind::CreditNote | DocKind::AdvanceTax))
            .map(str::to_owned),
        issue_date: doc.issue_date.as_deref().map(|d| date(d, lang)),
        tax_point_date: doc.tax_point_date.as_deref().map(|d| date(d, lang)),
        due_date: doc.due_date.as_deref().map(|d| date(d, lang)),
        supplier: supplier.map_or_else(
            || Party {
                name: match lang {
                    Lang::Cs => "Dodavatel nevyplněn".to_owned(),
                    Lang::En => "Supplier not set".to_owned(),
                },
                address: Vec::new(),
                ico: None,
                dic: None,
                registration: None,
                email: None,
            },
            |s| Party {
                name: s.name,
                address: s.address.lines().map(str::to_owned).collect(),
                ico: s.ico,
                dic: s.dic,
                registration: Some(s.registration).filter(|r| !r.is_empty()),
                email: s.email,
            },
        ),
        customer: Party {
            name: doc.customer.name.clone(),
            address: doc
                .customer
                .address
                .as_deref()
                .map(|a| {
                    a.split(['\n', ','])
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            ico: doc.customer.ico.clone(),
            dic: doc.customer.dic.clone(),
            registration: None,
            email: None,
        },
        lines,
        recap,
        base: fmt(doc.totals.base),
        vat: fmt(doc.totals.vat),
        gross: fmt(doc.totals.gross),
        advances: (deducted != 0).then(|| amount(deducted, currency, lang)),
        due: format!("{} {symbol}", fmt(due)).replace(' ', "\u{a0}"),
        currency: symbol,
        payment,
        spayd,
        reverse_charge: doc.lines.iter().any(|l| l.input.vat_code.starts_with("RC")),
        note: doc.note.clone(),
        pack: doc.pack.clone(),
    })
}

/// Amounts print with a true minus sign (U+2212).
fn amount(minor: i64, currency: Currency, lang: Lang) -> String {
    let cs = format_amount_cs(minor, currency).replace('-', "\u{2212}");
    match lang {
        Lang::Cs => cs,
        Lang::En => cs
            .chars()
            .map(|c| match c {
                '\u{a0}' => ',',
                ',' => '.',
                c => c,
            })
            .collect(),
    }
}

fn decimal_text(value: &str, lang: Lang) -> String {
    let value = value.replace('-', "\u{2212}");
    match lang {
        Lang::Cs => value.replace('.', ","),
        Lang::En => value,
    }
}

fn percent(rate: &str, lang: Lang) -> String {
    format!("{}\u{a0}%", decimal_text(rate, lang))
}

fn grouped(iban: &str) -> String {
    iban.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn date(iso: &str, lang: Lang) -> String {
    let parts: Vec<u32> = iso.split('-').filter_map(|p| p.parse().ok()).collect();
    let [y, m, d] = parts[..] else {
        return iso.to_owned();
    };
    match lang {
        Lang::Cs => format!("{d}.\u{a0}{m}.\u{a0}{y}"),
        Lang::En => {
            let month = MONTHS.get(m.saturating_sub(1) as usize).unwrap_or(&"");
            format!("{d}\u{a0}{month}\u{a0}{y}")
        }
    }
}
