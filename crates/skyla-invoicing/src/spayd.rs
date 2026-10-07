//! QR Platba: the Short Payment Descriptor (SPAYD 1.0) of the Czech Banking
//! Association, and the IBAN check it relies on.
//!
//! A descriptor looks like
//! `SPD*1.0*ACC:CZ5855000000001265098001*AM:480.50*CC:CZK*MSG:PLATBA ZA ZBOZI`.
//! Keys come in a fixed order so the same payment always gives the same
//! string (and the same QR code). An asterisk inside a value is written as
//! `%2A` and a percent sign as `%25`, as the format requires.

use skyla_money::Money;

use crate::InvoicingError;

/// Longest message (`MSG`) the format allows, in characters.
pub const MESSAGE_MAX: usize = 60;
/// Longest recipient name (`RN`) the format allows, in characters.
pub const RECIPIENT_MAX: usize = 35;

/// A payment to describe. Dates are ISO `YYYY-MM-DD`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentRequest {
    /// The payee's IBAN (spaces allowed; validated).
    pub iban: String,
    /// The payee bank's BIC, if it should travel with the IBAN.
    pub bic: Option<String>,
    /// The amount; must be positive.
    pub amount: Money,
    /// Due date.
    pub due_date: Option<String>,
    /// Payee name, cut to [`RECIPIENT_MAX`] characters.
    pub recipient: Option<String>,
    /// Message for the payee, cut to [`MESSAGE_MAX`] characters.
    pub message: Option<String>,
    /// Variable symbol: up to ten digits.
    pub variable_symbol: Option<String>,
}

/// Normalises an IBAN (upper case, no spaces) and checks its length and
/// ISO 7064 mod-97 check digits.
pub fn normalize_iban(input: &str) -> Result<String, InvoicingError> {
    let iban: String = input
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let bad = |why: &str| InvoicingError::Invalid(vec![format!("IBAN {input:?} {why}")]);
    if !(15..=34).contains(&iban.len()) {
        return Err(bad("has the wrong length"));
    }
    if !iban.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(bad("may hold only letters and digits"));
    }
    let bytes = iban.as_bytes();
    if !bytes[..2].iter().all(u8::is_ascii_uppercase) || !bytes[2..4].iter().all(u8::is_ascii_digit)
    {
        return Err(bad("must start with a country code and two check digits"));
    }
    if iban.starts_with("CZ") && iban.len() != 24 {
        return Err(bad("must have 24 characters for a Czech account"));
    }
    // Move the first four characters to the end; letters count as 10..35.
    let mut rest: u32 = 0;
    for c in iban[4..].chars().chain(iban[..4].chars()) {
        let value = c.to_digit(36).unwrap_or(0);
        rest = if value >= 10 {
            (rest * 100 + value) % 97
        } else {
            (rest * 10 + value) % 97
        };
    }
    if rest != 1 {
        return Err(bad("has wrong check digits"));
    }
    Ok(iban)
}

/// Builds the SPAYD string for a payment.
pub fn spayd(request: &PaymentRequest) -> Result<String, InvoicingError> {
    let mut problems = Vec::new();
    let iban = normalize_iban(&request.iban).map_err(|e| problems.push(e.to_string()));
    if request.amount.minor() <= 0 {
        problems.push("a QR payment needs a positive amount".to_owned());
    }
    let bic = request
        .bic
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty());
    if let Some(bic) = bic
        && !(bic.len() == 8 || bic.len() == 11)
    {
        problems.push(format!("BIC {bic:?} must have 8 or 11 characters"));
    }
    let vs = request
        .variable_symbol
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(vs) = vs
        && (vs.len() > 10 || !vs.chars().all(|c| c.is_ascii_digit()))
    {
        problems.push(format!("variable symbol {vs:?} must be up to ten digits"));
    }
    let due = match request.due_date.as_deref() {
        Some(d) => match compact_date(d) {
            Some(c) => Some(c),
            None => {
                problems.push(format!("due date {d:?} must be YYYY-MM-DD"));
                None
            }
        },
        None => None,
    };
    let Ok(iban) = iban else {
        return Err(InvoicingError::Invalid(problems));
    };
    if !problems.is_empty() {
        return Err(InvoicingError::Invalid(problems));
    }

    let mut out = format!("SPD*1.0*ACC:{iban}");
    if let Some(bic) = bic {
        out.push('+');
        out.push_str(&bic.to_ascii_uppercase());
    }
    out.push_str("*AM:");
    out.push_str(&decimal(request.amount));
    out.push_str("*CC:");
    out.push_str(request.amount.currency().code());
    if let Some(due) = due {
        out.push_str("*DT:");
        out.push_str(&due);
    }
    if let Some(name) = request
        .recipient
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        out.push_str("*RN:");
        out.push_str(&escape(name, RECIPIENT_MAX));
    }
    if let Some(msg) = request
        .message
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        out.push_str("*MSG:");
        out.push_str(&escape(msg, MESSAGE_MAX));
    }
    if let Some(vs) = vs {
        out.push_str("*X-VS:");
        out.push_str(vs);
    }
    Ok(out)
}

/// Splits a SPAYD string into its header check and key/value pairs, with
/// escapes decoded. Used to verify what a QR code carries.
pub fn parse_spayd(input: &str) -> Result<Vec<(String, String)>, InvoicingError> {
    let mut parts = input.split('*');
    if parts.next() != Some("SPD") || parts.next() != Some("1.0") {
        return Err(InvoicingError::Invalid(vec![
            "a SPAYD string starts with SPD*1.0".to_owned(),
        ]));
    }
    parts
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (key, value) = p.split_once(':').ok_or_else(|| {
                InvoicingError::Invalid(vec![format!("SPAYD field {p:?} has no key")])
            })?;
            Ok((key.to_owned(), unescape(value)))
        })
        .collect()
}

/// The variable symbol Czech practice derives from a document number: its
/// digits, keeping the last ten if there are more.
pub fn variable_symbol(number: &str) -> Option<String> {
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return None;
    }
    Some(digits[digits.len().saturating_sub(10)..].to_owned())
}

/// `480.50` for 48 050 minor units of a two-decimal currency.
fn decimal(amount: Money) -> String {
    let units = u32::from(amount.currency().minor_units());
    let per = 10_i64.pow(units);
    let minor = amount.minor();
    if units == 0 {
        return minor.to_string();
    }
    format!(
        "{}.{:0width$}",
        minor / per,
        minor % per,
        width = units as usize
    )
}

fn compact_date(iso: &str) -> Option<String> {
    let b = iso.as_bytes();
    let shape = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    shape.then(|| iso.replace('-', ""))
}

fn escape(value: &str, max: usize) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .fold(String::new(), |mut out, c| {
            match c {
                '%' => out.push_str("%25"),
                '*' => out.push_str("%2A"),
                c => out.push(c),
            }
            out
        })
}

fn unescape(value: &str) -> String {
    value
        .replace("%2A", "*")
        .replace("%2a", "*")
        .replace("%25", "%")
}

/// The payment a document asks for: `due` (what's left after advances) to
/// the supplier's account, with the document number as variable symbol.
/// `None` when there's nothing to pay by transfer: a draft, a credit note,
/// a tax document on a received advance, no IBAN, or nothing due.
pub fn document_payment(doc: &crate::Document, due: Money) -> Option<PaymentRequest> {
    let supplier = doc.supplier.as_ref()?;
    let number = doc.number.as_deref()?;
    let pays = matches!(doc.kind, crate::DocKind::Invoice | crate::DocKind::Advance);
    if !doc.issued || !pays || due.minor() <= 0 {
        return None;
    }
    Some(PaymentRequest {
        iban: supplier.iban.clone()?,
        bic: supplier.bic.clone(),
        amount: due,
        due_date: doc.due_date.clone(),
        recipient: Some(supplier.name.clone()),
        message: Some(format!("Faktura {number}")),
        variable_symbol: variable_symbol(number),
    })
}
