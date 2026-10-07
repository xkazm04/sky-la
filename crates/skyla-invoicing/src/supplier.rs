//! The supplier profile printed on every document: who issues it and where
//! to pay. Issuing a document snapshots the profile onto it, so a later
//! change of address or account never rewrites an issued invoice.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{InvoicingError, spayd::normalize_iban};

/// The issuing business.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Supplier {
    /// Name as registered.
    pub name: String,
    /// Company ID (IČO), eight digits.
    pub ico: Option<String>,
    /// VAT ID (DIČ), e.g. `CZ12345678`.
    pub dic: Option<String>,
    /// Postal address, lines separated by `\n`.
    pub address: String,
    /// Bank account as an IBAN; normalised on save.
    pub iban: Option<String>,
    /// The bank's BIC.
    pub bic: Option<String>,
    /// Contact e-mail.
    pub email: Option<String>,
    /// Whether the business is registered for VAT.
    pub vat_payer: bool,
    /// The registration line the law asks for, e.g. "Fyzická osoba
    /// zapsaná v živnostenském rejstříku".
    pub registration: String,
}

/// Checks an IČO: eight digits whose last is the mod-11 check digit.
pub fn valid_ico(ico: &str) -> bool {
    let digits: Vec<u32> = ico.chars().filter_map(|c| c.to_digit(10)).collect();
    if ico.len() != 8 || digits.len() != 8 {
        return false;
    }
    let sum: u32 = digits[..7]
        .iter()
        .zip((2..=8).rev())
        .map(|(d, w)| d * w)
        .sum();
    (11 - sum % 11) % 10 == digits[7]
}

/// Validates and stores the profile, listing every problem at once.
pub fn set_supplier(conn: &Connection, supplier: &Supplier) -> Result<Supplier, InvoicingError> {
    let mut problems = Vec::new();
    let mut s = supplier.clone();
    s.name = s.name.trim().to_owned();
    if s.name.is_empty() {
        problems.push("the supplier needs a name".to_owned());
    }
    if s.address.trim().is_empty() {
        problems.push("the supplier needs an address".to_owned());
    }
    s.ico = s.ico.map(|i| i.trim().to_owned()).filter(|i| !i.is_empty());
    if let Some(ico) = &s.ico
        && !valid_ico(ico)
    {
        problems.push(format!("IČO {ico} isn't valid"));
    }
    s.dic = s
        .dic
        .map(|d| d.trim().replace(' ', "").to_ascii_uppercase())
        .filter(|d| !d.is_empty());
    if let Some(dic) = &s.dic {
        let ok = dic.len() >= 4
            && dic.as_bytes()[..2].iter().all(u8::is_ascii_uppercase)
            && dic.len() <= 14;
        if !ok {
            problems.push(format!("DIČ {dic} isn't valid"));
        }
    }
    if s.vat_payer && s.dic.is_none() {
        problems.push("a VAT payer needs a DIČ".to_owned());
    }
    s.iban = match s.iban.as_deref().map(str::trim).filter(|i| !i.is_empty()) {
        Some(iban) => match normalize_iban(iban) {
            Ok(n) => Some(n),
            Err(e) => {
                problems.push(e.to_string());
                None
            }
        },
        None => None,
    };
    s.bic = s
        .bic
        .map(|b| b.trim().to_ascii_uppercase())
        .filter(|b| !b.is_empty());
    s.email = s
        .email
        .map(|e| e.trim().to_owned())
        .filter(|e| !e.is_empty());
    if !problems.is_empty() {
        return Err(InvoicingError::Invalid(problems));
    }
    conn.execute(
        "INSERT INTO supplier_profile (id, profile) VALUES (1, ?1)
         ON CONFLICT (id) DO UPDATE SET profile = excluded.profile",
        params![to_json(&s)?],
    )?;
    Ok(s)
}

/// The current profile, if one was saved.
pub fn supplier(conn: &Connection) -> Result<Option<Supplier>, InvoicingError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT profile FROM supplier_profile WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    json.as_deref().map(from_json).transpose()
}

pub(crate) fn to_json(s: &Supplier) -> Result<String, InvoicingError> {
    serde_json::to_string(s).map_err(|e| InvoicingError::Invalid(vec![e.to_string()]))
}

pub(crate) fn from_json(json: &str) -> Result<Supplier, InvoicingError> {
    serde_json::from_str(json)
        .map_err(|e| InvoicingError::Invalid(vec![format!("stored supplier profile: {e}")]))
}
