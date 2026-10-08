//! Reading issued invoices exported from Pohoda (XML) and Fakturoid (CSV)
//! (WP-31). The parsers only read: they turn a file into documents with
//! their totals per VAT rate and list what they couldn't read. Posting
//! happens in the core, through the kernel, after the user saw a preview.
//!
//! Pohoda: the XML data pack (`dat:dataPack`, or a `rsp:responsePack`
//! from an export), invoices matched by local name so either envelope
//! works; `priceHigh`/`priceLow` are the standard and reduced rates,
//! `priceNone` supplies without VAT. Fakturoid: the CSV export, columns
//! found by their header (Czech or English names), semicolon or comma,
//! UTF-8 or Windows-1250, decimal comma or point. When a CSV gives only
//! totals, the core finds the pack rate that reproduces its VAT.
//!
//! Credit notes are read too, with the number of the invoice they correct.
//! Programs disagree on the sign of a credit note's amounts, so the reader
//! normalises them to the way this crate issues its own: every base, VAT
//! and total negative. A credit note whose amounts mix signs is refused.

use serde::{Deserialize, Serialize};

use crate::{Customer, DocKind};

/// Which program a file came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Pohoda (Stormware), XML.
    Pohoda,
    /// Fakturoid, CSV.
    Fakturoid,
}

impl Source {
    /// For people.
    pub fn name(self) -> &'static str {
        match self {
            Self::Pohoda => "Pohoda",
            Self::Fakturoid => "Fakturoid",
        }
    }
}

/// The rate band of a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    /// The standard rate.
    Standard,
    /// The reduced rate.
    Reduced,
    /// No VAT on it.
    None,
    /// VAT charged at a rate the file doesn't name; the core finds the
    /// pack rate that gives exactly this VAT.
    Unstated,
}

/// A document's base and VAT in one band, in minor units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    /// Which band.
    pub band: Band,
    /// Base.
    pub base_minor: i64,
    /// VAT.
    pub vat_minor: i64,
}

/// One issued document as read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Imported {
    /// Where in the file (item or row), for messages.
    pub position: usize,
    /// Invoice or credit note.
    pub kind: DocKind,
    /// Its number in the other program.
    pub number: String,
    /// Issued.
    pub issue_date: String,
    /// DUZP.
    pub tax_point_date: Option<String>,
    /// Due.
    pub due_date: Option<String>,
    /// The customer.
    pub customer: Customer,
    /// What it was for, when the file says.
    pub description: Option<String>,
    /// Totals per band.
    pub parts: Vec<Part>,
    /// The total as the file states it (negative on a credit note).
    pub total_minor: i64,
    /// Currency code.
    pub currency: String,
    /// A credit note's invoice: its number in the other program.
    pub original_number: Option<String>,
}

/// What a file held.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportFile {
    /// The program.
    pub source: Source,
    /// Documents read.
    pub documents: Vec<Imported>,
    /// What couldn't be read, and where.
    pub problems: Vec<String>,
}

/// Which format a file is, by its content.
pub fn detect(bytes: &[u8]) -> Option<Source> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_lowercase();
    if head.contains("stormware.cz/schema")
        || head.contains("<dat:datapack")
        || head.contains("responsepack")
    {
        Some(Source::Pohoda)
    } else if head.contains(';') || head.contains(',') {
        Some(Source::Fakturoid)
    } else {
        None
    }
}

/// `12 100,50`, `12100.50`, `-1 210` → minor units.
fn amount(text: &str) -> Option<i64> {
    let t: String = text
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{a0}' && *c != '\'')
        .collect();
    if t.is_empty() {
        return Some(0);
    }
    let (neg, body) = match t.strip_prefix('-') {
        Some(r) => (true, r.to_owned()),
        None => (false, t.clone()),
    };
    // The last separator is the decimal one when two digits or fewer follow.
    let sep = body.rfind([',', '.']);
    let (whole, frac) = match sep {
        Some(i) if body.len() - i - 1 <= 2 => {
            (body[..i].replace([',', '.'], ""), body[i + 1..].to_owned())
        }
        _ => (body.replace([',', '.'], ""), String::new()),
    };
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let w: i64 = whole.parse().ok()?;
    let f: i64 = if frac.is_empty() {
        0
    } else {
        format!("{frac:0<2}").parse().ok()?
    };
    let v = w.checked_mul(100)?.checked_add(f)?;
    Some(if neg { -v } else { v })
}

/// Gives a credit note's amounts the sign this crate's own credit notes
/// have: negative. Returns false when they mix signs, which no credit note
/// does.
fn credit_signs(parts: &mut [Part], total: &mut i64) -> bool {
    let (mut positive, mut negative) = (false, false);
    for v in parts
        .iter()
        .flat_map(|p| [p.base_minor, p.vat_minor])
        .chain([*total])
    {
        positive |= v > 0;
        negative |= v < 0;
    }
    if positive && negative {
        return false;
    }
    if positive {
        for p in parts.iter_mut() {
            p.base_minor = p.base_minor.saturating_neg();
            p.vat_minor = p.vat_minor.saturating_neg();
        }
        *total = total.saturating_neg();
    }
    true
}

/// `2026-01-15`, `15.01.2026`, `15. 1. 2026` → `2026-01-15`.
fn date(text: &str) -> Option<String> {
    let t = text.trim();
    if t.len() >= 10 && t.as_bytes().get(4) == Some(&b'-') {
        return Some(t[..10].to_owned());
    }
    let parts: Vec<&str> = t
        .split('.')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if let [d, m, y] = parts.as_slice() {
        let (d, m, y): (u32, u32, u32) =
            (d.parse().ok()?, m.parse().ok()?, y.get(..4)?.parse().ok()?);
        return Some(format!("{y:04}-{m:02}-{d:02}"));
    }
    None
}

fn opt(text: &str) -> Option<String> {
    let t = text.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

// ── Pohoda ────────────────────────────────────────────────────────────────

fn child<'a, 'i>(n: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    n.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

/// The document number a Pohoda reference element holds: its `number`
/// (itself maybe wrapping `numberRequested` or `ids`), else the element's
/// own `numberRequested`, `ids` or text.
fn document_ref(n: roxmltree::Node<'_, '_>) -> String {
    let named = |node: roxmltree::Node<'_, '_>| {
        let requested = text_of(child(node, "numberRequested"));
        if !requested.is_empty() {
            return requested;
        }
        let ids = text_of(child(node, "ids"));
        if ids.is_empty() {
            text_of(Some(node))
        } else {
            ids
        }
    };
    let inner = child(n, "number").map(named).unwrap_or_default();
    if inner.is_empty() { named(n) } else { inner }
}

fn text_of(n: Option<roxmltree::Node<'_, '_>>) -> String {
    n.and_then(|n| n.text())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

/// Reads a Pohoda XML export.
pub fn parse_pohoda(bytes: &[u8]) -> ImportFile {
    let mut out = ImportFile {
        source: Source::Pohoda,
        documents: Vec::new(),
        problems: Vec::new(),
    };
    let text = match decode(bytes) {
        Ok(t) => t,
        Err(e) => {
            out.problems.push(e);
            return out;
        }
    };
    let options = roxmltree::ParsingOptions {
        allow_dtd: false,
        nodes_limit: 2_000_000,
        ..Default::default()
    };
    let doc = match roxmltree::Document::parse_with_options(&text, options) {
        Ok(d) => d,
        Err(e) => {
            out.problems
                .push(format!("not a readable Pohoda XML file: {e}"));
            return out;
        }
    };
    let invoices = doc.descendants().filter(|n| {
        n.is_element() && n.tag_name().name() == "invoice" && child(*n, "invoiceHeader").is_some()
    });
    for (i, inv) in invoices.enumerate() {
        let position = i + 1;
        let Some(header) = child(inv, "invoiceHeader") else {
            continue;
        };
        let kind = match text_of(child(header, "invoiceType")).as_str() {
            "issuedInvoice" => DocKind::Invoice,
            "issuedCreditNotice" => DocKind::CreditNote,
            other => {
                let what = match other {
                    "receivedInvoice" | "receivedCreditNotice" => "a received document",
                    "issuedAdvanceInvoice" | "receivedAdvanceInvoice" => "an advance invoice",
                    "issuedCorrectiveTax" | "receivedCorrectiveTax" => "a corrective tax document",
                    _ => "not an issued invoice",
                };
                out.problems.push(format!(
                    "item {position} is {what}; only issued invoices and credit notes are imported"
                ));
                continue;
            }
        };
        let number_node = child(header, "number");
        let number = {
            let requested = text_of(number_node.and_then(|n| child(n, "numberRequested")));
            if requested.is_empty() {
                text_of(number_node.and_then(|n| child(n, "ids")))
            } else {
                requested
            }
        };
        let issue = date(&text_of(child(header, "date")));
        let address = child(header, "partnerIdentity").and_then(|p| child(p, "address"));
        let field = |name: &str| text_of(address.and_then(|a| child(a, name)));
        let company = field("company");
        let name = if company.is_empty() {
            field("name")
        } else {
            company
        };
        let street_city = [
            field("street"),
            [field("zip"), field("city")].join(" ").trim().to_owned(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
        let summary = child(inv, "invoiceSummary").and_then(|s| child(s, "homeCurrency"));
        let money = |name: &str| amount(&text_of(summary.and_then(|s| child(s, name))));
        let (Some(none), Some(low), Some(low_vat), Some(high), Some(high_vat)) = (
            money("priceNone"),
            money("priceLow"),
            money("priceLowVAT"),
            money("priceHigh"),
            money("priceHighVAT"),
        ) else {
            out.problems.push(format!(
                "invoice {position} ({number}): its totals aren't readable"
            ));
            continue;
        };
        if number.is_empty() || issue.is_none() || name.is_empty() {
            out.problems.push(format!(
                "invoice {position}: needs a number, a date and a customer"
            ));
            continue;
        }
        let mut parts: Vec<Part> = [
            (Band::Standard, high, high_vat),
            (Band::Reduced, low, low_vat),
            (Band::None, none, 0),
        ]
        .into_iter()
        .filter(|(_, b, v)| *b != 0 || *v != 0)
        .map(|(band, base_minor, vat_minor)| Part {
            band,
            base_minor,
            vat_minor,
        })
        .collect();
        let mut total_minor = parts.iter().map(|p| p.base_minor + p.vat_minor).sum();
        let original_number = if kind == DocKind::CreditNote {
            // Pohoda names the source document of a credit note in the header.
            ["sourceDocument", "originalDocument", "relatedInvoice"]
                .into_iter()
                .find_map(|name| {
                    let r = document_ref(child(header, name)?);
                    (!r.is_empty()).then_some(r)
                })
        } else {
            None
        };
        if kind == DocKind::CreditNote && !credit_signs(&mut parts, &mut total_minor) {
            out.problems.push(format!(
                "credit note {position} ({number}): its amounts have mixed signs"
            ));
            continue;
        }
        out.documents.push(Imported {
            position,
            kind,
            number,
            issue_date: issue.unwrap_or_default(),
            tax_point_date: date(&text_of(child(header, "dateTax"))),
            due_date: date(&text_of(child(header, "dateDue"))),
            customer: Customer {
                name,
                ico: opt(&field("ico")),
                dic: opt(&field("dic")),
                address: opt(&street_city),
            },
            description: opt(&text_of(child(header, "text"))),
            parts,
            total_minor,
            currency: "CZK".into(),
            original_number,
        });
    }
    if out.documents.is_empty() && out.problems.is_empty() {
        out.problems.push("the file has no issued invoices".into());
    }
    out
}

fn decode(bytes: &[u8]) -> Result<String, String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(t) => Ok(t.to_owned()),
        Err(_) => {
            let (text, _, bad) = encoding_rs::WINDOWS_1250.decode(bytes);
            if bad {
                Err("the file is neither UTF-8 nor Windows-1250".into())
            } else {
                Ok(text.into_owned())
            }
        }
    }
}

// ── Fakturoid ─────────────────────────────────────────────────────────────

/// Header names each column goes by (lower case, without diacritics marks
/// compared as written).
const COLUMNS: &[(&str, &[&str])] = &[
    (
        "number",
        &["číslo", "číslo faktury", "number", "invoice number"],
    ),
    (
        "issued",
        &["vystaveno", "datum vystavení", "issued on", "issued"],
    ),
    (
        "taxable",
        &[
            "duzp",
            "datum zdanitelného plnění",
            "zdanitelné plnění",
            "taxable fulfillment due",
        ],
    ),
    ("due", &["splatnost", "datum splatnosti", "due on", "due"]),
    (
        "client",
        &[
            "odběratel",
            "klient",
            "client",
            "client name",
            "název odběratele",
        ],
    ),
    (
        "ico",
        &["ičo", "ič", "client registration no", "registration no"],
    ),
    ("dic", &["dič", "client vat no", "vat no"]),
    ("street", &["ulice", "street", "client street"]),
    ("city", &["město", "city", "client city"]),
    ("zip", &["psč", "zip", "client zip"]),
    ("currency", &["měna", "currency"]),
    ("base21", &["základ 21 %", "základ 21%", "subtotal 21 %"]),
    ("vat21", &["dph 21 %", "dph 21%", "vat 21 %"]),
    ("base12", &["základ 12 %", "základ 12%", "subtotal 12 %"]),
    ("vat12", &["dph 12 %", "dph 12%", "vat 12 %"]),
    (
        "base0",
        &[
            "osvobozeno",
            "základ 0 %",
            "bez dph (osvobozeno)",
            "subtotal 0 %",
        ],
    ),
    ("base", &["bez dph", "celkem bez dph", "subtotal", "základ"]),
    ("vat", &["dph", "celkem dph", "vat"]),
    ("total", &["celkem", "celkem s dph", "total"]),
    ("type", &["typ", "druh dokladu", "document type"]),
    (
        "original",
        &[
            "původní doklad",
            "původní faktura",
            "číslo původní faktury",
            "číslo původního dokladu",
            "opravovaný doklad",
            "opravuje",
            "k faktuře",
            "original invoice",
            "original invoice number",
            "original document",
            "corrected invoice",
            "credited invoice",
            "related invoice",
        ],
    ),
    ("subject", &["předmět", "popis", "subject", "description"]),
];

/// Reads a Fakturoid CSV export.
pub fn parse_fakturoid(bytes: &[u8]) -> ImportFile {
    let mut out = ImportFile {
        source: Source::Fakturoid,
        documents: Vec::new(),
        problems: Vec::new(),
    };
    let text = match decode(bytes) {
        Ok(t) => t,
        Err(e) => {
            out.problems.push(e);
            return out;
        }
    };
    let first = text.lines().next().unwrap_or_default();
    let delimiter = if first.matches(';').count() >= first.matches(',').count() {
        b';'
    } else {
        b','
    };
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = match reader.headers() {
        Ok(h) => h.iter().map(|s| s.trim().to_lowercase()).collect(),
        Err(e) => {
            out.problems.push(format!("not a readable CSV file: {e}"));
            return out;
        }
    };
    let index = |column: &str| -> Option<usize> {
        let names = COLUMNS
            .iter()
            .find(|(c, _)| *c == column)
            .map(|(_, n)| *n)?;
        headers.iter().position(|h| names.contains(&h.as_str()))
    };
    let (Some(number_at), Some(issued_at), Some(client_at)) =
        (index("number"), index("issued"), index("client"))
    else {
        out.problems.push(
            "the CSV needs number, issue date and client columns (see the Fakturoid profile)"
                .into(),
        );
        return out;
    };
    for (i, record) in reader.records().enumerate() {
        let row = i + 2;
        let Ok(record) = record else {
            out.problems.push(format!("row {row}: not readable"));
            continue;
        };
        let get = |column: &str| {
            index(column)
                .and_then(|c| record.get(c))
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        let number = record.get(number_at).unwrap_or_default().trim().to_owned();
        if number.is_empty() {
            continue;
        }
        let Some(issued) = date(record.get(issued_at).unwrap_or_default()) else {
            out.problems.push(format!(
                "row {row} ({number}): the issue date isn't readable"
            ));
            continue;
        };
        let money = |column: &str| amount(&get(column));
        let mut parts = Vec::new();
        let banded = [
            ("base21", "vat21", Band::Standard),
            ("base12", "vat12", Band::Reduced),
        ];
        let has_bands = banded.iter().any(|(b, _, _)| index(b).is_some());
        let mut unreadable = false;
        if has_bands {
            for (b, v, band) in banded {
                match (money(b), money(v)) {
                    (Some(base), Some(vat)) if base != 0 || vat != 0 => parts.push(Part {
                        band,
                        base_minor: base,
                        vat_minor: vat,
                    }),
                    (Some(_), Some(_)) => {}
                    _ => unreadable = true,
                }
            }
            match money("base0") {
                Some(0) => {}
                Some(base) => parts.push(Part {
                    band: Band::None,
                    base_minor: base,
                    vat_minor: 0,
                }),
                None => unreadable = true,
            }
        } else {
            // Totals only: the core finds the rate with the pack.
            match (money("base"), money("vat")) {
                (Some(base), Some(0)) => parts.push(Part {
                    band: Band::None,
                    base_minor: base,
                    vat_minor: 0,
                }),
                (Some(base), Some(vat)) => parts.push(Part {
                    band: Band::Unstated,
                    base_minor: base,
                    vat_minor: vat,
                }),
                _ => unreadable = true,
            }
        }
        if unreadable || parts.is_empty() {
            out.problems
                .push(format!("row {row} ({number}): its amounts aren't readable"));
            continue;
        }
        let address = [
            get("street"),
            [get("zip"), get("city")].join(" ").trim().to_owned(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
        let mut total_minor = money("total")
            .unwrap_or_else(|| parts.iter().map(|p| p.base_minor + p.vat_minor).sum());
        let original_number = opt(&get("original"));
        let kind_text = get("type").to_lowercase();
        let kind = if ["oprav", "dobropis", "credit"]
            .iter()
            .any(|w| kind_text.contains(w))
            || original_number.is_some()
            || total_minor < 0
        {
            DocKind::CreditNote
        } else {
            DocKind::Invoice
        };
        if kind == DocKind::CreditNote && !credit_signs(&mut parts, &mut total_minor) {
            out.problems.push(format!(
                "row {row} ({number}): a credit note whose amounts have mixed signs"
            ));
            continue;
        }
        out.documents.push(Imported {
            position: row,
            kind,
            number,
            issue_date: issued,
            tax_point_date: date(&get("taxable")),
            due_date: date(&get("due")),
            customer: Customer {
                name: record.get(client_at).unwrap_or_default().trim().to_owned(),
                ico: opt(&get("ico")),
                dic: opt(&get("dic")),
                address: opt(&address),
            },
            description: opt(&get("subject")),
            parts,
            total_minor,
            currency: opt(&get("currency")).unwrap_or_else(|| "CZK".into()),
            original_number: if kind == DocKind::CreditNote {
                original_number
            } else {
                None
            },
        });
    }
    if out.documents.is_empty() && out.problems.is_empty() {
        out.problems.push("the file has no invoices".into());
    }
    out
}

/// Reads a file of either kind.
pub fn parse(bytes: &[u8]) -> Option<ImportFile> {
    Some(match detect(bytes)? {
        Source::Pohoda => parse_pohoda(bytes),
        Source::Fakturoid => parse_fakturoid(bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_and_dates_in_both_conventions() {
        assert_eq!(amount("12 100,50"), Some(1_210_050));
        assert_eq!(amount("12100.50"), Some(1_210_050));
        assert_eq!(amount("1.210,00"), Some(121_000));
        assert_eq!(amount("-605"), Some(-60_500));
        assert_eq!(
            amount("12,345"),
            Some(1_234_500),
            "three digits after a comma are thousands"
        );
        assert_eq!(amount("abc"), None);
        assert_eq!(date("15.01.2026").as_deref(), Some("2026-01-15"));
        assert_eq!(date("5. 1. 2026").as_deref(), Some("2026-01-05"));
        assert_eq!(date("2026-01-15T00:00:00").as_deref(), Some("2026-01-15"));
    }

    const POHODA: &[u8] =
        include_bytes!("../../../packages/fixtures/data/imports/pohoda-faktury.xml");
    const FAKTUROID: &[u8] =
        include_bytes!("../../../packages/fixtures/data/imports/fakturoid-faktury.csv");

    #[test]
    fn reads_the_pohoda_sample() {
        assert_eq!(detect(POHODA), Some(Source::Pohoda));
        let f = parse(POHODA).expect("detected");
        let numbers: Vec<&str> = f.documents.iter().map(|d| d.number.as_str()).collect();
        assert_eq!(
            numbers,
            ["2026-038", "2026-041", "2026-044", "2026-047", "2026-049"]
        );
        assert_eq!(
            f.problems.len(),
            1,
            "the received invoice is skipped: {:?}",
            f.problems
        );
        let d = &f.documents[2];
        assert_eq!(d.customer.name, "Studio Brno s.r.o.");
        assert_eq!(d.customer.ico.as_deref(), Some("94722188"));
        assert_eq!(
            d.customer.address.as_deref(),
            Some("Masarykova 12, 602 00 Brno")
        );
        assert_eq!(d.description.as_deref(), Some("Návrh vizuální identity"));
        assert_eq!(
            d.parts,
            [Part {
                band: Band::Standard,
                base_minor: 1_800_000,
                vat_minor: 378_000
            }]
        );
        assert_eq!(d.total_minor, 2_178_000);
        assert_eq!(d.due_date.as_deref(), Some("2026-05-04"));
    }

    #[test]
    fn reads_the_fakturoid_sample() {
        assert_eq!(detect(FAKTUROID), Some(Source::Fakturoid));
        let f = parse(FAKTUROID).expect("detected");
        assert!(f.problems.is_empty(), "{:?}", f.problems);
        assert_eq!(f.documents.len(), 3);
        let d = &f.documents[0];
        assert_eq!(
            (d.number.as_str(), d.issue_date.as_str()),
            ("2026-051", "2026-05-12")
        );
        assert_eq!(
            d.parts,
            [Part {
                band: Band::Unstated,
                base_minor: 500_000,
                vat_minor: 105_000
            }]
        );
        assert_eq!(d.total_minor, 605_000);
        assert_eq!(f.documents[2].kind, DocKind::CreditNote);
        // This credit note says nothing about its invoice; its amounts come
        // out negative, as the invoicing crate's own credit notes have them.
        assert_eq!(f.documents[2].original_number, None);
        assert_eq!(f.documents[2].total_minor, -121_000);
        assert_eq!(f.documents[0].original_number, None);
    }

    const POHODA_CREDIT: &[u8] =
        include_bytes!("../../../packages/fixtures/data/imports/pohoda-dobropisy.xml");
    const FAKTUROID_CREDIT: &[u8] =
        include_bytes!("../../../packages/fixtures/data/imports/fakturoid-dobropisy.csv");

    fn standard(base_minor: i64, vat_minor: i64) -> Vec<Part> {
        vec![Part {
            band: Band::Standard,
            base_minor,
            vat_minor,
        }]
    }

    #[test]
    fn reads_pohoda_credit_notes_with_their_invoice_and_negative_signs() {
        let f = parse(POHODA_CREDIT).expect("detected");
        assert!(f.problems.is_empty(), "{:?}", f.problems);
        let got: Vec<(&str, DocKind, Option<&str>, i64)> = f
            .documents
            .iter()
            .map(|d| {
                (
                    d.number.as_str(),
                    d.kind,
                    d.original_number.as_deref(),
                    d.total_minor,
                )
            })
            .collect();
        // Totals are base + VAT in minor units: -(10 000 + 2 100) = -1 210 000.
        assert_eq!(
            got,
            [
                (
                    "OD2026-001",
                    DocKind::CreditNote,
                    Some("2026-102"),
                    -1_210_000
                ),
                // Stated positive (40 000 + 8 400 = 48 400): normalised to negative.
                (
                    "OD2026-002",
                    DocKind::CreditNote,
                    Some("2026-102"),
                    -4_840_000
                ),
                (
                    "OD2026-003",
                    DocKind::CreditNote,
                    Some("2026-114"),
                    -121_000
                ),
                (
                    "OD2026-004",
                    DocKind::CreditNote,
                    Some("2026-999"),
                    -121_000
                ),
                // The reference sits in `ids` here, in `numberRequested` above.
                ("DB26-001", DocKind::CreditNote, Some("2026-130"), -484_000),
                ("2026-130", DocKind::Invoice, None, 1_210_000),
            ]
        );
        assert_eq!(f.documents[0].parts, standard(-1_000_000, -210_000));
        assert_eq!(f.documents[1].parts, standard(-4_000_000, -840_000));
    }

    #[test]
    fn reads_fakturoid_credit_notes_by_type_and_original_column() {
        let f = parse(FAKTUROID_CREDIT).expect("detected");
        assert!(f.problems.is_empty(), "{:?}", f.problems);
        let got: Vec<(&str, DocKind, Option<&str>, i64)> = f
            .documents
            .iter()
            .map(|d| {
                (
                    d.number.as_str(),
                    d.kind,
                    d.original_number.as_deref(),
                    d.total_minor,
                )
            })
            .collect();
        // 5 000,00 + 1 050,00 = 6 050,00, stated positive, kept negative.
        assert_eq!(
            got,
            [
                ("2026-140", DocKind::Invoice, None, 2_420_000),
                (
                    "OD2026-010",
                    DocKind::CreditNote,
                    Some("2026-140"),
                    -605_000
                ),
                (
                    "OD2026-011",
                    DocKind::CreditNote,
                    Some("2026-140"),
                    -2_420_000
                ),
                (
                    "OD2026-012",
                    DocKind::CreditNote,
                    Some("2026-041"),
                    -121_000
                ),
            ]
        );
        assert_eq!(
            f.documents[1].parts,
            [Part {
                band: Band::Unstated,
                base_minor: -500_000,
                vat_minor: -105_000
            }]
        );
    }

    #[test]
    fn english_headers_name_a_credit_note_and_its_invoice() {
        let csv = "Number,Document type,Issued on,Client,Original invoice,Subtotal,VAT,Total
                   A-7,Invoice,2026-06-01,Dvořák s.r.o.,,\"1,000.00\",210.00,\"1,210.00\"
                   CN-1,Credit note,2026-06-02,Dvořák s.r.o.,A-7,\"1,000.00\",210.00,\"1,210.00\"
                   CN-2,Credit note,2026-06-03,Dvořák s.r.o.,A-7,\"1,000.00\",-210.00,\"790.00\"
";
        let f = parse_fakturoid(csv.as_bytes());
        assert_eq!(f.documents.len(), 2, "{:?}", f.documents);
        assert_eq!(f.documents[0].kind, DocKind::Invoice);
        let cn = &f.documents[1];
        assert_eq!(cn.kind, DocKind::CreditNote);
        assert_eq!(cn.original_number.as_deref(), Some("A-7"));
        assert_eq!(cn.parts[0].base_minor, -100_000);
        assert_eq!(cn.parts[0].vat_minor, -21_000);
        assert_eq!(cn.total_minor, -121_000);
        // 1 000,00 base with -210,00 VAT: no credit note has mixed signs.
        assert_eq!(f.problems.len(), 1, "{:?}", f.problems);
        assert!(f.problems[0].contains("CN-2") && f.problems[0].contains("mixed signs"));
    }

    #[test]
    fn czech_original_header_alone_makes_a_credit_note() {
        // No type column; the invoice number column says what it is.
        let csv = "Číslo;Vystaveno;Odběratel;Číslo původní faktury;Bez DPH;DPH;Celkem
                   OD-9;03.06.2026;Dvořák s.r.o.;A-7;100,00;21,00;121,00
";
        let f = parse_fakturoid(csv.as_bytes());
        assert!(f.problems.is_empty(), "{:?}", f.problems);
        assert_eq!(f.documents[0].kind, DocKind::CreditNote);
        assert_eq!(f.documents[0].original_number.as_deref(), Some("A-7"));
        assert_eq!(f.documents[0].total_minor, -12_100);
    }

    #[test]
    fn a_pohoda_credit_note_without_a_source_document_has_no_original() {
        let xml = String::from_utf8_lossy(POHODA_CREDIT).replace(
            "<inv:sourceDocument><typ:number>2026-999</typ:number></inv:sourceDocument>",
            "",
        );
        let f = parse_pohoda(xml.as_bytes());
        let d = f
            .documents
            .iter()
            .find(|d| d.number == "OD2026-004")
            .expect("read");
        assert_eq!(
            (d.kind, d.original_number.as_deref()),
            (DocKind::CreditNote, None)
        );
    }

    #[test]
    fn windows_1250_and_comma_separated_english_headers() {
        let text = "Number,Issued on,Client,Subtotal,VAT,Total\nA-7,2026-06-01,Dvořák s.r.o.,\"1,000.00\",210.00,\"1,210.00\"\n";
        let (bytes, _, _) = encoding_rs::WINDOWS_1250.encode(text);
        let f = parse_fakturoid(&bytes);
        assert!(f.problems.is_empty(), "{:?}", f.problems);
        assert_eq!(f.documents[0].customer.name, "Dvořák s.r.o.");
        assert_eq!(f.documents[0].parts[0].base_minor, 100_000);
    }

    #[test]
    fn hostile_input_is_refused_not_panicked_on() {
        let dtd = b"<?xml version=\"1.0\"?><!DOCTYPE x [<!ENTITY a \"aaaa\">]><dat:dataPack xmlns:dat=\"http://www.stormware.cz/schema/version_2/data.xsd\">&a;</dat:dataPack>";
        assert!(!parse_pohoda(dtd).problems.is_empty());
        assert!(!parse_fakturoid(b"just;a;header\n").problems.is_empty());
        assert!(parse(b"").is_none());
    }
}
