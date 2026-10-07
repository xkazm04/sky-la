//! UN/CEFACT Cross Industry Invoice D16B in the EN 16931 profile
//! (`urn:cen.eu:en16931:2017`), the syntax behind Factur-X and ZUGFeRD.
//! Unlike Peppol it needs no electronic address, so it also serves
//! suppliers outside VAT.

use skyla_money::Money;
use skyla_rules::Pack;

use crate::address::split_address;
use crate::exchange::{ExportInput, compact_date, en16931};
use crate::spayd::variable_symbol;
use crate::xml::{Xml, decimal};
use crate::{DocKind, InvoicingError};

/// The EN 16931 specification identifier (BT-24).
pub const GUIDELINE_ID: &str = "urn:cen.eu:en16931:2017";

const RSM: &str = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100";
const RAM: &str =
    "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100";
const UDT: &str = "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100";
const QDT: &str = "urn:un:unece:uncefact:data:standard:QualifiedDataType:100";

/// Writes an issued document as CII D16B (EN 16931).
pub fn to_cii(pack: &Pack, input: ExportInput<'_>) -> Result<String, InvoicingError> {
    let en = en16931(pack, input)?;
    let doc = input.doc;
    let supplier = doc
        .supplier
        .as_ref()
        .ok_or_else(|| InvoicingError::Invalid(vec!["no supplier".into()]))?;
    let vat_ids = en.taxes.iter().all(|t| t.category != "O");
    let number = doc.number.as_deref().unwrap_or_default();
    let date = |x: &mut Xml, element: &str, d: &str| {
        x.open(element);
        x.leaf_with("udt:DateTimeString", &[("format", "102")], &compact_date(d));
        x.close();
    };
    let plain = |m: Money| decimal(m);

    let mut x = Xml::new();
    x.open_with(
        "rsm:CrossIndustryInvoice",
        &[
            ("xmlns:rsm", RSM),
            ("xmlns:ram", RAM),
            ("xmlns:udt", UDT),
            ("xmlns:qdt", QDT),
        ],
    );
    x.open("rsm:ExchangedDocumentContext");
    x.open("ram:GuidelineSpecifiedDocumentContextParameter");
    x.leaf("ram:ID", GUIDELINE_ID);
    x.close();
    x.close();

    x.open("rsm:ExchangedDocument");
    x.leaf("ram:ID", number);
    x.leaf("ram:TypeCode", en.type_code);
    date(
        &mut x,
        "ram:IssueDateTime",
        doc.issue_date.as_deref().unwrap_or_default(),
    );
    if !doc.note.trim().is_empty() {
        x.open("ram:IncludedNote");
        x.leaf("ram:Content", doc.note.trim());
        x.close();
    }
    x.close();

    x.open("rsm:SupplyChainTradeTransaction");
    for l in &en.lines {
        x.open("ram:IncludedSupplyChainTradeLineItem");
        x.open("ram:AssociatedDocumentLineDocument");
        x.leaf("ram:LineID", &l.id);
        x.close();
        x.open("ram:SpecifiedTradeProduct");
        x.leaf("ram:Name", &l.name);
        x.close();
        x.open("ram:SpecifiedLineTradeAgreement");
        x.open("ram:NetPriceProductTradePrice");
        x.leaf("ram:ChargeAmount", &plain(l.price));
        x.close();
        x.close();
        x.open("ram:SpecifiedLineTradeDelivery");
        x.leaf_with(
            "ram:BilledQuantity",
            &[("unitCode", l.unit_code)],
            &l.quantity,
        );
        x.close();
        x.open("ram:SpecifiedLineTradeSettlement");
        x.open("ram:ApplicableTradeTax");
        x.leaf("ram:TypeCode", "VAT");
        x.leaf("ram:CategoryCode", &l.category);
        if let Some(rate) = l.rate {
            x.leaf("ram:RateApplicablePercent", &rate.normalize().to_string());
        }
        x.close();
        x.open("ram:SpecifiedTradeSettlementLineMonetarySummation");
        x.leaf("ram:LineTotalAmount", &plain(l.net));
        x.close();
        x.close();
        x.close();
    }

    x.open("ram:ApplicableHeaderTradeAgreement");
    party(
        &mut x,
        "ram:SellerTradeParty",
        &Party {
            name: &supplier.name,
            company_id: supplier.ico.as_deref(),
            address: &supplier.address,
            email: supplier.email.as_deref(),
            vat_id: supplier.dic.as_deref().filter(|_| vat_ids && en.vat_payer),
        },
    );
    party(
        &mut x,
        "ram:BuyerTradeParty",
        &Party {
            name: &doc.customer.name,
            company_id: doc.customer.ico.as_deref(),
            address: doc.customer.address.as_deref().unwrap_or_default(),
            email: None,
            vat_id: doc.customer.dic.as_deref().filter(|_| vat_ids),
        },
    );
    x.close();

    x.open("ram:ApplicableHeaderTradeDelivery");
    if let Some(tp) = &doc.tax_point_date {
        x.open("ram:ActualDeliverySupplyChainEvent");
        date(&mut x, "ram:OccurrenceDateTime", tp);
        x.close();
    }
    x.close();

    x.open("ram:ApplicableHeaderTradeSettlement");
    let pays = doc.kind != DocKind::CreditNote && en.payable.minor() > 0;
    if pays && let Some(vs) = variable_symbol(number) {
        x.leaf("ram:PaymentReference", &vs);
    }
    x.leaf("ram:InvoiceCurrencyCode", &doc.currency);
    if pays && let Some(iban) = &supplier.iban {
        x.open("ram:SpecifiedTradeSettlementPaymentMeans");
        x.leaf("ram:TypeCode", "30");
        x.open("ram:PayeePartyCreditorFinancialAccount");
        x.leaf("ram:IBANID", iban);
        x.close();
        if let Some(bic) = &supplier.bic {
            x.open("ram:PayeeSpecifiedCreditorFinancialInstitution");
            x.leaf("ram:BICID", bic);
            x.close();
        }
        x.close();
    }
    for t in &en.taxes {
        x.open("ram:ApplicableTradeTax");
        x.leaf("ram:CalculatedAmount", &plain(t.tax));
        x.leaf("ram:TypeCode", "VAT");
        if let Some(reason) = &t.exemption_reason {
            x.leaf("ram:ExemptionReason", reason);
        }
        x.leaf("ram:BasisAmount", &plain(t.taxable));
        x.leaf("ram:CategoryCode", &t.category);
        if let Some(rate) = t.rate {
            x.leaf("ram:RateApplicablePercent", &rate.normalize().to_string());
        }
        x.close();
    }
    if pays && let Some(due) = &doc.due_date {
        x.open("ram:SpecifiedTradePaymentTerms");
        date(&mut x, "ram:DueDateDateTime", due);
        x.close();
    }
    x.open("ram:SpecifiedTradeSettlementHeaderMonetarySummation");
    x.leaf("ram:LineTotalAmount", &plain(en.line_total));
    x.leaf("ram:TaxBasisTotalAmount", &plain(en.line_total));
    x.amount("ram:TaxTotalAmount", en.tax_total);
    x.leaf("ram:GrandTotalAmount", &plain(en.tax_inclusive));
    if en.prepaid.minor() != 0 {
        x.leaf("ram:TotalPrepaidAmount", &plain(en.prepaid));
    }
    x.leaf("ram:DuePayableAmount", &plain(en.payable));
    x.close();
    if let Some(original) = input.related.filter(|_| doc.kind == DocKind::CreditNote) {
        x.open("ram:InvoiceReferencedDocument");
        x.leaf(
            "ram:IssuerAssignedID",
            original.number.as_deref().unwrap_or_default(),
        );
        if let Some(d) = &original.issue_date {
            x.open("ram:FormattedIssueDateTime");
            x.leaf_with("qdt:DateTimeString", &[("format", "102")], &compact_date(d));
            x.close();
        }
        x.close();
    }
    x.close();
    x.close();
    Ok(x.finish())
}

struct Party<'a> {
    name: &'a str,
    company_id: Option<&'a str>,
    address: &'a str,
    email: Option<&'a str>,
    vat_id: Option<&'a str>,
}

fn party(x: &mut Xml, element: &str, p: &Party<'_>) {
    let a = split_address(p.address);
    x.open(element);
    x.leaf("ram:Name", p.name);
    if let Some(id) = p.company_id {
        x.open("ram:SpecifiedLegalOrganization");
        x.leaf("ram:ID", id);
        x.close();
    }
    x.open("ram:PostalTradeAddress");
    x.leaf_opt("ram:PostcodeCode", &a.postal_zone);
    x.leaf_opt("ram:LineOne", &format!("{} {}", a.street, a.building));
    x.leaf_opt("ram:CityName", &a.city);
    x.leaf("ram:CountryID", &a.country);
    x.close();
    if let Some(email) = p.email {
        x.open("ram:URIUniversalCommunication");
        x.leaf_with("ram:URIID", &[("schemeID", "EM")], email);
        x.close();
    }
    if let Some(vat) = p.vat_id {
        x.open("ram:SpecifiedTaxRegistration");
        x.leaf_with("ram:ID", &[("schemeID", "VA")], vat);
        x.close();
    }
    x.close();
}
