//! ISDOC 6.0.2, the Czech e-invoice format (`http://isdoc.cz/namespace/2013`).
//!
//! The writer maps an issued document onto the schema's required structure:
//! parties with IČO, DIČ and a split postal address, one `InvoiceLine` per
//! line with its VAT, the recapitulation per rate (advances already taxed
//! appear as "already claimed"), the totals and the bank-transfer payment.
//! Amounts are the books' amounts; nothing is recomputed here.

use skyla_money::Money;
use skyla_rules::Pack;

use crate::address::{country_name_cs, split_address_for};
use crate::exchange::{ExportInput, PreparedLine, czech_account, prepare};
use crate::spayd::variable_symbol;
use crate::xml::{Xml, decimal};
use crate::{DocKind, InvoicingError};

/// The ISDOC version written.
pub const VERSION: &str = "6.0.2";
/// The ISDOC namespace.
pub const NAMESPACE: &str = "http://isdoc.cz/namespace/2013";

/// Writes an issued document as ISDOC 6.0.2 XML.
pub fn to_isdoc(pack: &Pack, input: ExportInput<'_>) -> Result<String, InvoicingError> {
    let p = prepare(pack, input)?;
    let doc = input.doc;
    let supplier = doc
        .supplier
        .as_ref()
        .ok_or_else(|| InvoicingError::Invalid(vec!["no supplier".into()]))?;
    let currency = doc.currency.as_str();
    let zero = Money::zero(doc.totals.gross.currency());
    let amount = |m: Money| decimal(m);

    let mut x = Xml::new();
    x.open_with("Invoice", &[("xmlns", NAMESPACE), ("version", VERSION)]);
    x.leaf(
        "DocumentType",
        match doc.kind {
            DocKind::Invoice => "1",
            DocKind::CreditNote => "2",
            DocKind::Advance => "4",
            DocKind::AdvanceTax => "5",
        },
    );
    x.leaf("ID", doc.number.as_deref().unwrap_or_default());
    x.leaf("UUID", &doc.uid);
    x.leaf("IssuingSystem", "sky-la");
    x.leaf("IssueDate", doc.issue_date.as_deref().unwrap_or_default());
    if doc.kind != DocKind::Advance
        && let Some(tp) = &doc.tax_point_date
    {
        x.leaf("TaxPointDate", tp);
    }
    x.leaf("VATApplicable", bool_text(p.vat_payer));
    x.leaf("ElectronicPossibilityAgreementReference", "");
    // ISDOC has no field for why no Czech VAT is charged on a supply to
    // another member state, so the statement goes in the note.
    let statements: Vec<&str> = doc.tax_notes.iter().map(|n| n.cs.as_str()).collect();
    let note = std::iter::once(doc.note.trim())
        .chain(statements)
        .filter(|n| !n.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if !note.is_empty() {
        x.leaf("Note", &note);
    }
    x.leaf("LocalCurrencyCode", currency);
    x.leaf("CurrRate", "1");
    x.leaf("RefCurrRate", "1");

    x.open("AccountingSupplierParty");
    party(
        &mut x,
        &Party {
            id: supplier.ico.as_deref(),
            name: &supplier.name,
            address: &supplier.address,
            dic: supplier.dic.as_deref(),
            email: supplier.email.as_deref(),
        },
    );
    x.close();
    x.open("AccountingCustomerParty");
    party(
        &mut x,
        &Party {
            id: doc.customer.ico.as_deref(),
            name: &doc.customer.name,
            address: doc.customer.address.as_deref().unwrap_or_default(),
            dic: doc.customer.dic.as_deref(),
            email: None,
        },
    );
    x.close();

    if let Some(related) = input.related {
        x.open("OriginalDocumentReferences");
        x.open_with("OriginalDocumentReference", &[("id", "original")]);
        x.leaf("ID", related.number.as_deref().unwrap_or_default());
        if let Some(d) = &related.issue_date {
            x.leaf("IssueDate", d);
        }
        x.leaf("UUID", &related.uid);
        x.close();
        x.close();
    }

    x.open("InvoiceLines");
    for l in &p.lines {
        line(&mut x, l, p.vat_payer, input.related.is_some());
    }
    x.close();

    // Advances already taxed: each tax document's base and gross per rate.
    if !input.advances.is_empty() {
        x.open("TaxedDeposits");
        for adv in input.advances {
            let number = adv.number.as_deref().unwrap_or_default();
            let on = adv.tax_point_date.as_deref().unwrap_or_default();
            for r in &adv.totals.recap {
                x.open("TaxedDeposit");
                x.leaf("ID", number);
                x.leaf(
                    "VariableSymbol",
                    &variable_symbol(number).unwrap_or_default(),
                );
                x.leaf("TaxableDepositAmount", &amount(r.base));
                x.leaf(
                    "TaxInclusiveDepositAmount",
                    &amount(r.base.checked_add(r.vat)?),
                );
                x.open("ClassifiedTaxCategory");
                x.leaf(
                    "Percent",
                    &pack.vat_rate(&r.vat_code, on)?.normalize().to_string(),
                );
                x.leaf("VATCalculationMethod", "0");
                x.leaf("VATApplicable", bool_text(p.vat_payer));
                x.close();
                x.close();
            }
        }
        x.close();
    }

    x.open("TaxTotal");
    for r in &p.recap {
        x.open("TaxSubTotal");
        let gross = r.base.checked_add(r.vat)?;
        let claimed_gross = r.claimed_base.checked_add(r.claimed_vat)?;
        x.leaf("TaxableAmount", &amount(r.base));
        x.leaf("TaxAmount", &amount(r.vat));
        x.leaf("TaxInclusiveAmount", &amount(gross));
        x.leaf("AlreadyClaimedTaxableAmount", &amount(r.claimed_base));
        x.leaf("AlreadyClaimedTaxAmount", &amount(r.claimed_vat));
        x.leaf("AlreadyClaimedTaxInclusiveAmount", &amount(claimed_gross));
        x.leaf(
            "DifferenceTaxableAmount",
            &amount(r.base.checked_sub(r.claimed_base)?),
        );
        x.leaf(
            "DifferenceTaxAmount",
            &amount(r.vat.checked_sub(r.claimed_vat)?),
        );
        x.leaf(
            "DifferenceTaxInclusiveAmount",
            &amount(gross.checked_sub(claimed_gross)?),
        );
        x.open("TaxCategory");
        x.leaf("Percent", &r.rate.normalize().to_string());
        x.leaf(
            "VATApplicable",
            bool_text(p.vat_payer && !r.outside_vat && !r.eu_supply),
        );
        x.close();
        x.close();
    }
    x.leaf("TaxAmount", &amount(p.vat));
    x.close();

    x.open("LegalMonetaryTotal");
    x.leaf("TaxExclusiveAmount", &amount(p.base));
    x.leaf("TaxInclusiveAmount", &amount(p.gross));
    x.leaf("AlreadyClaimedTaxExclusiveAmount", &amount(p.claimed_base));
    x.leaf("AlreadyClaimedTaxInclusiveAmount", &amount(p.claimed_gross));
    x.leaf(
        "DifferenceTaxExclusiveAmount",
        &amount(p.base.checked_sub(p.claimed_base)?),
    );
    x.leaf("DifferenceTaxInclusiveAmount", &amount(p.payable));
    x.leaf("PaidDepositsAmount", &amount(zero));
    x.leaf("PayableAmount", &amount(p.payable));
    x.close();

    let pays = matches!(doc.kind, DocKind::Invoice | DocKind::Advance);
    if pays
        && p.payable.minor() > 0
        && let Some(iban) = &supplier.iban
    {
        let (account, bank) = czech_account(iban).unwrap_or_default();
        x.open("PaymentMeans");
        x.open("Payment");
        x.leaf("PaidAmount", &amount(p.payable));
        x.leaf("PaymentMeansCode", "42");
        x.open("Details");
        x.leaf(
            "PaymentDueDate",
            doc.due_date.as_deref().unwrap_or_default(),
        );
        x.leaf("ID", &account);
        x.leaf("BankCode", &bank);
        x.leaf("Name", "");
        x.leaf("IBAN", iban);
        x.leaf("BIC", supplier.bic.as_deref().unwrap_or_default());
        if let Some(vs) = doc.number.as_deref().and_then(variable_symbol) {
            x.leaf("VariableSymbol", &vs);
        }
        x.close();
        x.close();
        x.close();
    }
    Ok(x.finish())
}

struct Party<'a> {
    id: Option<&'a str>,
    name: &'a str,
    address: &'a str,
    dic: Option<&'a str>,
    email: Option<&'a str>,
}

fn party(x: &mut Xml, p: &Party<'_>) {
    let a = split_address_for(p.address, p.dic);
    x.open("Party");
    x.open("PartyIdentification");
    x.leaf("ID", p.id.unwrap_or_default());
    x.close();
    x.open("PartyName");
    x.leaf("Name", p.name);
    x.close();
    x.open("PostalAddress");
    x.leaf("StreetName", &a.street);
    x.leaf("BuildingNumber", &a.building);
    x.leaf("CityName", &a.city);
    x.leaf("PostalZone", &a.postal_zone);
    x.open("Country");
    x.leaf("IdentificationCode", &a.country);
    x.leaf("Name", country_name_cs(&a.country));
    x.close();
    x.close();
    if let Some(dic) = p.dic {
        x.open("PartyTaxScheme");
        x.leaf("CompanyID", dic);
        x.leaf("TaxScheme", "VAT");
        x.close();
    }
    if let Some(email) = p.email {
        x.open("Contact");
        x.leaf("ElectronicMail", email);
        x.close();
    }
    x.close();
}

fn line(x: &mut Xml, l: &PreparedLine, vat_payer: bool, has_original: bool) {
    x.open("InvoiceLine");
    x.leaf("ID", &l.no.to_string());
    if has_original {
        x.open_with("OriginalDocumentReference", &[("ref", "original")]);
        x.close();
    }
    let unit = isdoc_unit(&l.unit);
    if unit.is_empty() {
        x.leaf("InvoicedQuantity", &l.quantity);
    } else {
        x.leaf_with("InvoicedQuantity", &[("unitCode", unit)], &l.quantity);
    }
    x.leaf("LineExtensionAmount", &decimal(l.base));
    x.leaf("LineExtensionAmountTaxInclusive", &decimal(l.gross));
    x.leaf("LineExtensionTaxAmount", &decimal(l.vat));
    x.leaf("UnitPrice", &decimal(l.unit_price));
    x.leaf("UnitPriceTaxInclusive", &decimal(l.unit_price_gross));
    x.open("ClassifiedTaxCategory");
    x.leaf("Percent", &l.rate.normalize().to_string());
    // 0: VAT computed from the base ("zdola"), as the books do.
    x.leaf("VATCalculationMethod", "0");
    x.leaf(
        "VATApplicable",
        bool_text(vat_payer && !l.outside_vat && !l.eu_supply),
    );
    x.close();
    x.open("Item");
    x.leaf("Description", &l.description);
    x.close();
    x.close();
}

/// UN/ECE Rec. 20 codes for the units the app uses; others pass through.
pub(crate) fn isdoc_unit(unit: &str) -> &str {
    match unit.trim() {
        "h" | "hod" | "hodina" => "HUR",
        "ks" | "pc" | "pcs" | "kus" => "H87",
        "den" | "day" | "d" => "DAY",
        "měs" | "month" => "MON",
        "km" => "KMT",
        "kg" => "KGM",
        "" => "",
        other => other,
    }
}

fn bool_text(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}
