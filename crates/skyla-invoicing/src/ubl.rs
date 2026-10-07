//! UBL 2.1 following Peppol BIS Billing 3.0 (EN 16931): an `Invoice` for
//! invoices and tax documents on received advances, a `CreditNote` for
//! credit notes.
//!
//! Peppol needs an electronic address for both parties; sky-la uses the VAT
//! number under its country's EAS scheme (`9929` for a Czech DIČ), so both
//! parties need one. Documents of suppliers outside VAT go out as CII.

use skyla_rules::Pack;

use crate::address::split_address;
use crate::exchange::{En16931, EnTax, ExportInput, en16931, vat_endpoint_scheme};
use crate::spayd::variable_symbol;
use crate::xml::Xml;
use crate::{DocKind, InvoicingError};

/// Peppol BIS Billing 3.0 customization (BT-24).
pub const CUSTOMIZATION_ID: &str =
    "urn:cen.eu:en16931:2017#compliant#urn:fdc:peppol.eu:2017:poacc:billing:3.0";
/// Peppol BIS Billing 3.0 profile (BT-23).
pub const PROFILE_ID: &str = "urn:fdc:peppol.eu:2017:poacc:billing:01:1.0";

const CAC: &str = "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2";
const CBC: &str = "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2";

/// Writes an issued document as UBL 2.1 (Peppol BIS Billing 3.0).
pub fn to_ubl(pack: &Pack, input: ExportInput<'_>) -> Result<String, InvoicingError> {
    let en = en16931(pack, input)?;
    let doc = input.doc;
    let supplier = doc
        .supplier
        .as_ref()
        .ok_or_else(|| InvoicingError::Invalid(vec!["no supplier".into()]))?;
    // Category O forbids VAT identifiers (BR-O-02).
    let vat_ids = en.taxes.iter().all(|t| t.category != "O");
    let mut problems = Vec::new();
    let seller_endpoint = endpoint(supplier.dic.as_deref());
    let buyer_endpoint = endpoint(doc.customer.dic.as_deref());
    if seller_endpoint.is_none() {
        problems.push(
            "Peppol needs the supplier's electronic address, taken from a VAT number (DIČ); export CII instead"
                .to_owned(),
        );
    }
    if buyer_endpoint.is_none() {
        problems.push(
            "Peppol needs the customer's electronic address, taken from their VAT number (DIČ)"
                .to_owned(),
        );
    }
    if !problems.is_empty() {
        return Err(InvoicingError::Invalid(problems));
    }
    let credit = doc.kind == DocKind::CreditNote;
    let (root, ns, line_el, qty_el) = if credit {
        (
            "CreditNote",
            "urn:oasis:names:specification:ubl:schema:xsd:CreditNote-2",
            "cac:CreditNoteLine",
            "cbc:CreditedQuantity",
        )
    } else {
        (
            "Invoice",
            "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
            "cac:InvoiceLine",
            "cbc:InvoicedQuantity",
        )
    };
    let number = doc.number.as_deref().unwrap_or_default();

    let mut x = Xml::new();
    x.open_with(
        root,
        &[("xmlns", ns), ("xmlns:cac", CAC), ("xmlns:cbc", CBC)],
    );
    x.leaf("cbc:CustomizationID", CUSTOMIZATION_ID);
    x.leaf("cbc:ProfileID", PROFILE_ID);
    x.leaf("cbc:ID", number);
    x.leaf(
        "cbc:IssueDate",
        doc.issue_date.as_deref().unwrap_or_default(),
    );
    // The two document schemas order these differently.
    if credit {
        if let Some(tp) = &doc.tax_point_date {
            x.leaf("cbc:TaxPointDate", tp);
        }
        x.leaf("cbc:CreditNoteTypeCode", en.type_code);
        x.leaf_opt("cbc:Note", &doc.note);
    } else {
        x.leaf_opt("cbc:DueDate", doc.due_date.as_deref().unwrap_or_default());
        x.leaf("cbc:InvoiceTypeCode", en.type_code);
        x.leaf_opt("cbc:Note", &doc.note);
        if let Some(tp) = &doc.tax_point_date {
            x.leaf("cbc:TaxPointDate", tp);
        }
    }
    x.leaf("cbc:DocumentCurrencyCode", &doc.currency);
    // Peppol asks for a buyer reference or an order reference (R003); the
    // document number stands in until documents carry the buyer's own.
    x.leaf("cbc:BuyerReference", number);
    if let Some(original) = input.related.filter(|_| credit) {
        x.open("cac:BillingReference");
        x.open("cac:InvoiceDocumentReference");
        x.leaf("cbc:ID", original.number.as_deref().unwrap_or_default());
        x.leaf_opt(
            "cbc:IssueDate",
            original.issue_date.as_deref().unwrap_or_default(),
        );
        x.close();
        x.close();
    }

    x.open("cac:AccountingSupplierParty");
    party(
        &mut x,
        &Party {
            endpoint: seller_endpoint,
            name: &supplier.name,
            address: &supplier.address,
            vat_id: supplier.dic.as_deref().filter(|_| vat_ids && en.vat_payer),
            company_id: supplier.ico.as_deref(),
            email: supplier.email.as_deref(),
        },
    );
    x.close();
    x.open("cac:AccountingCustomerParty");
    party(
        &mut x,
        &Party {
            endpoint: buyer_endpoint,
            name: &doc.customer.name,
            address: doc.customer.address.as_deref().unwrap_or_default(),
            vat_id: doc.customer.dic.as_deref().filter(|_| vat_ids),
            company_id: doc.customer.ico.as_deref(),
            email: None,
        },
    );
    x.close();

    if !credit
        && en.payable.minor() > 0
        && let Some(iban) = &supplier.iban
    {
        x.open("cac:PaymentMeans");
        x.leaf_with("cbc:PaymentMeansCode", &[("name", "Credit transfer")], "30");
        if let Some(vs) = variable_symbol(number) {
            x.leaf("cbc:PaymentID", &vs);
        }
        x.open("cac:PayeeFinancialAccount");
        x.leaf("cbc:ID", iban);
        x.leaf("cbc:Name", &supplier.name);
        if let Some(bic) = &supplier.bic {
            x.open("cac:FinancialInstitutionBranch");
            x.leaf("cbc:ID", bic);
            x.close();
        }
        x.close();
        x.close();
    }

    x.open("cac:TaxTotal");
    x.amount("cbc:TaxAmount", en.tax_total);
    for t in &en.taxes {
        x.open("cac:TaxSubtotal");
        x.amount("cbc:TaxableAmount", t.taxable);
        x.amount("cbc:TaxAmount", t.tax);
        tax_category(&mut x, "cac:TaxCategory", t);
        x.close();
    }
    x.close();

    monetary_total(&mut x, &en);

    for l in &en.lines {
        x.open(line_el);
        x.leaf("cbc:ID", &l.id);
        x.leaf_with(qty_el, &[("unitCode", l.unit_code)], &l.quantity);
        x.amount("cbc:LineExtensionAmount", l.net);
        x.open("cac:Item");
        x.leaf("cbc:Name", &l.name);
        x.open("cac:ClassifiedTaxCategory");
        x.leaf("cbc:ID", &l.category);
        if let Some(rate) = l.rate {
            x.leaf("cbc:Percent", &rate.normalize().to_string());
        }
        x.open("cac:TaxScheme");
        x.leaf("cbc:ID", "VAT");
        x.close();
        x.close();
        x.close();
        x.open("cac:Price");
        x.amount("cbc:PriceAmount", l.price);
        x.close();
        x.close();
    }
    Ok(x.finish())
}

fn monetary_total(x: &mut Xml, en: &En16931) {
    x.open("cac:LegalMonetaryTotal");
    x.amount("cbc:LineExtensionAmount", en.line_total);
    x.amount("cbc:TaxExclusiveAmount", en.line_total);
    x.amount("cbc:TaxInclusiveAmount", en.tax_inclusive);
    if en.prepaid.minor() != 0 {
        x.amount("cbc:PrepaidAmount", en.prepaid);
    }
    x.amount("cbc:PayableAmount", en.payable);
    x.close();
}

fn tax_category(x: &mut Xml, element: &str, t: &EnTax) {
    x.open(element);
    x.leaf("cbc:ID", &t.category);
    if let Some(rate) = t.rate {
        x.leaf("cbc:Percent", &rate.normalize().to_string());
    }
    if let Some(reason) = &t.exemption_reason {
        x.leaf("cbc:TaxExemptionReason", reason);
    }
    x.open("cac:TaxScheme");
    x.leaf("cbc:ID", "VAT");
    x.close();
    x.close();
}

struct Party<'a> {
    endpoint: Option<(&'static str, &'a str)>,
    name: &'a str,
    address: &'a str,
    vat_id: Option<&'a str>,
    company_id: Option<&'a str>,
    email: Option<&'a str>,
}

fn endpoint(vat_id: Option<&str>) -> Option<(&'static str, &str)> {
    let id = vat_id?;
    Some((vat_endpoint_scheme(id)?, id))
}

fn party(x: &mut Xml, p: &Party<'_>) {
    let a = split_address(p.address);
    x.open("cac:Party");
    if let Some((scheme, id)) = p.endpoint {
        x.leaf_with("cbc:EndpointID", &[("schemeID", scheme)], id);
    }
    x.open("cac:PartyName");
    x.leaf("cbc:Name", p.name);
    x.close();
    x.open("cac:PostalAddress");
    let street = format!("{} {}", a.street, a.building);
    x.leaf_opt("cbc:StreetName", &street);
    x.leaf_opt("cbc:CityName", &a.city);
    x.leaf_opt("cbc:PostalZone", &a.postal_zone);
    x.open("cac:Country");
    x.leaf("cbc:IdentificationCode", &a.country);
    x.close();
    x.close();
    if let Some(vat) = p.vat_id {
        x.open("cac:PartyTaxScheme");
        x.leaf("cbc:CompanyID", vat);
        x.open("cac:TaxScheme");
        x.leaf("cbc:ID", "VAT");
        x.close();
        x.close();
    }
    x.open("cac:PartyLegalEntity");
    x.leaf("cbc:RegistrationName", p.name);
    if let Some(id) = p.company_id {
        x.leaf("cbc:CompanyID", id);
    }
    x.close();
    if let Some(email) = p.email {
        x.open("cac:Contact");
        x.leaf("cbc:ElectronicMail", email);
        x.close();
    }
    x.close();
}
