//! Documents and their lifecycle: draft → issued (numbered, posted, frozen),
//! then paid or credited as the ledger's settlement links say.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use skyla_ledger::{
    NewEntry, NewLine, Replay, SourceKind, create_draft_as as create_entry_as, functional_currency,
    get_entry, link_settlement, post_entry_at, settlements_of,
};
use skyla_money::{Currency, Money, Rate, vat};
use skyla_rules::Pack;

use crate::{
    InvoicingError,
    supplier::{self, Supplier},
};

/// What a document is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    /// A tax invoice (daňový doklad).
    Invoice,
    /// A credit note (opravný daňový doklad) against an issued invoice.
    CreditNote,
    /// A request for an advance payment (zálohová faktura); not a tax document.
    Advance,
    /// The tax document for a received advance (daňový doklad k přijaté platbě).
    AdvanceTax,
}

impl DocKind {
    /// Stored name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invoice => "invoice",
            Self::CreditNote => "credit_note",
            Self::Advance => "advance",
            Self::AdvanceTax => "advance_tax",
        }
    }

    fn from_db(s: &str) -> Result<Self, InvoicingError> {
        Ok(match s {
            "invoice" => Self::Invoice,
            "credit_note" => Self::CreditNote,
            "advance" => Self::Advance,
            "advance_tax" => Self::AdvanceTax,
            other => {
                return Err(InvoicingError::Invalid(vec![format!(
                    "unknown kind {other}"
                )]));
            }
        })
    }

    /// Whether issuing posts to the ledger.
    pub fn posts(self) -> bool {
        self != Self::Advance
    }
}

/// The ledger accounts invoicing posts to. A chart property, not statutory data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accounts {
    /// Trade receivables.
    pub receivables: String,
    /// VAT.
    pub vat: String,
    /// Advances received.
    pub advances_received: String,
    /// Default revenue account for lines without one.
    pub revenue: String,
}

impl Accounts {
    /// The CZ chart: 311, 343, 324, 602.
    pub fn cz() -> Self {
        Self {
            receivables: "311".into(),
            vat: "343".into(),
            advances_received: "324".into(),
            revenue: "602".into(),
        }
    }
}

/// Who the document is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Customer {
    /// Legal name.
    pub name: String,
    /// IČO.
    pub ico: Option<String>,
    /// DIČ.
    pub dic: Option<String>,
    /// Postal address, one line.
    pub address: Option<String>,
}

/// One line as entered. For an advance tax document the unit price is the
/// amount received, VAT included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineInput {
    /// What was supplied.
    pub description: String,
    /// Quantity, a decimal such as `56` or `1.5`; negative on credit notes.
    pub quantity: String,
    /// Unit, e.g. `h`.
    pub unit: String,
    /// Price per unit excluding VAT, in minor units.
    pub unit_price_minor: i64,
    /// A VAT code from the rule pack.
    pub vat_code: String,
    /// Revenue account; the default when absent.
    pub account: Option<String>,
}

/// A draft as entered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftInput {
    /// What it is.
    pub kind: DocKind,
    /// Number series code.
    pub series: String,
    /// The customer.
    pub customer: Customer,
    /// Due date.
    pub due_date: Option<String>,
    /// Tax point (DUZP); the issue date when absent.
    pub tax_point_date: Option<String>,
    /// Free text.
    pub note: String,
    /// The lines.
    pub lines: Vec<LineInput>,
    /// A credit note's invoice, or an advance tax document's advance.
    pub related_id: Option<i64>,
    /// Advance tax documents a final invoice deducts.
    pub advances: Vec<i64>,
}

/// One line with its computed base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    /// One-based.
    pub line_no: i64,
    /// As entered.
    pub input: LineInput,
    /// Quantity × unit price (VAT included for an advance tax document).
    pub amount: Money,
}

/// VAT recapitulation for one code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VatRecap {
    /// The code.
    pub vat_code: String,
    /// Its rate on the tax point, in percent.
    pub rate: String,
    /// Base.
    pub base: Money,
    /// VAT.
    pub vat: Money,
}

/// Document totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Totals {
    /// Per VAT code, by code.
    pub recap: Vec<VatRecap>,
    /// Sum of bases.
    pub base: Money,
    /// Sum of VAT.
    pub vat: Money,
    /// Base plus VAT.
    pub gross: Money,
}

/// A stored document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Document {
    /// Row id.
    pub id: i64,
    /// Stable id.
    pub uid: String,
    /// What it is.
    pub kind: DocKind,
    /// Number series.
    pub series: String,
    /// True once issued.
    pub issued: bool,
    /// The number, once issued.
    pub number: Option<String>,
    /// Issue date.
    pub issue_date: Option<String>,
    /// Tax point.
    pub tax_point_date: Option<String>,
    /// Due date.
    pub due_date: Option<String>,
    /// The customer.
    pub customer: Customer,
    /// Currency.
    pub currency: String,
    /// Free text.
    pub note: String,
    /// Credit note → invoice; advance tax document → advance.
    pub related_id: Option<i64>,
    /// Advance tax documents it deducts.
    pub advances: Vec<i64>,
    /// The ledger entry, once issued (none for advances).
    pub entry_id: Option<i64>,
    /// `cz-2026@2026.1`: the pack its totals were computed with.
    pub pack: Option<String>,
    /// The lines.
    pub lines: Vec<Line>,
    /// Totals: stored at issue, computed live for drafts.
    pub totals: Totals,
    /// The supplier: snapshotted at issue, the current profile for drafts.
    pub supplier: Option<Supplier>,
}

/// Where an issued document stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Settlement {
    /// Not issued yet.
    Draft,
    /// Nothing settled.
    Open,
    /// Partly paid or credited.
    PartlySettled,
    /// Fully paid (possibly with a partial credit).
    Paid,
    /// Fully credited, nothing paid.
    Credited,
}

/// What's paid, credited and open on a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocState {
    /// Total.
    pub gross: Money,
    /// Advances deducted on a final invoice.
    pub advances: Money,
    /// Paid by payments.
    pub paid: Money,
    /// Credited by credit notes.
    pub credited: Money,
    /// Still to pay.
    pub open: Money,
    /// Summary.
    pub settlement: Settlement,
    /// Date of the last payment or credit, when settled.
    pub settled_on: Option<String>,
}

fn invalid(problems: Vec<String>) -> InvoicingError {
    InvoicingError::Invalid(problems)
}

/// Starts a savepoint; the returned guard rolls back unless committed.
pub(crate) fn atomically<T>(
    conn: &Connection,
    f: impl FnOnce(&Connection) -> Result<T, InvoicingError>,
) -> Result<T, InvoicingError> {
    conn.execute_batch("SAVEPOINT invoicing_op")?;
    match f(conn) {
        Ok(v) => {
            conn.execute_batch("RELEASE invoicing_op")?;
            Ok(v)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK TO invoicing_op; RELEASE invoicing_op");
            Err(e)
        }
    }
}

/// Defines a number series. `pattern` uses `{YYYY}` or `{YY}` for the year
/// and `{N…}` for the counter, its width the number of `N`s: `{YYYY}-{NNN}`.
pub fn define_series(
    conn: &Connection,
    code: &str,
    kind: DocKind,
    pattern: &str,
    description: &str,
) -> Result<(), InvoicingError> {
    if !pattern.contains("{N") || !pattern.contains('}') {
        return Err(invalid(vec![format!(
            "series pattern {pattern:?} needs a {{N…}} counter"
        )]));
    }
    conn.execute(
        "INSERT INTO doc_series (code, kind, pattern, description) VALUES (?1, ?2, ?3, ?4)",
        params![code, kind.as_str(), pattern, description],
    )?;
    Ok(())
}

fn series(conn: &Connection, code: &str) -> Result<(DocKind, String), InvoicingError> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT kind, pattern FROM doc_series WHERE code = ?1",
            [code],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (kind, pattern) = row.ok_or_else(|| InvoicingError::UnknownSeries(code.to_owned()))?;
    Ok((DocKind::from_db(&kind)?, pattern))
}

/// Formats a number from a series pattern.
pub fn format_number(pattern: &str, year: i64, seq: i64) -> String {
    let mut out = pattern
        .replace("{YYYY}", &format!("{year:04}"))
        .replace("{YY}", &format!("{:02}", year % 100));
    if let (Some(start), Some(len)) = (
        out.find("{N"),
        out.find("{N").and_then(|s| out[s..].find('}')),
    ) {
        let width = len - 1;
        out.replace_range(start..start + len + 1, &format!("{seq:0width$}"));
    }
    out
}

fn date_year(date: &str) -> Result<i64, InvoicingError> {
    if !skyla_ledger::is_iso_date(date) {
        return Err(invalid(vec![format!("{date:?} isn't a YYYY-MM-DD date")]));
    }
    date.get(..4)
        .and_then(|y| y.parse().ok())
        .ok_or_else(|| invalid(vec![format!("{date:?} has no year")]))
}

fn validate(input: &DraftInput, series_kind: DocKind) -> Vec<String> {
    let mut problems = Vec::new();
    if series_kind != input.kind {
        problems.push(format!(
            "series {} is for {}, not {}",
            input.series,
            series_kind.as_str(),
            input.kind.as_str()
        ));
    }
    if input.customer.name.trim().is_empty() {
        problems.push("the customer needs a name".into());
    }
    if input.lines.is_empty() {
        problems.push("a document needs at least one line".into());
    }
    for (i, l) in input.lines.iter().enumerate() {
        let n = i + 1;
        if l.description.trim().is_empty() {
            problems.push(format!("line {n}: needs a description"));
        }
        match l.quantity.parse::<Rate>() {
            Ok(q) if q.is_zero() => problems.push(format!("line {n}: quantity is zero")),
            Ok(q) if input.kind == DocKind::CreditNote && q.is_sign_positive() => {
                problems.push(format!("line {n}: a credit note's quantities are negative"));
            }
            Ok(q) if input.kind != DocKind::CreditNote && q.is_sign_negative() => {
                problems.push(format!("line {n}: quantity must be positive"));
            }
            Ok(_) => {}
            Err(_) => problems.push(format!(
                "line {n}: quantity {:?} isn't a number",
                l.quantity
            )),
        }
        if l.unit_price_minor < 0 {
            problems.push(format!("line {n}: unit price can't be negative"));
        }
    }
    match (input.kind, input.related_id) {
        (DocKind::CreditNote | DocKind::AdvanceTax, None) => {
            problems.push(format!(
                "a {} must name the document it belongs to",
                input.kind.as_str()
            ));
        }
        (DocKind::Invoice | DocKind::Advance, Some(_)) => problems
            .push("only credit notes and advance tax documents relate to another document".into()),
        _ => {}
    }
    if input.kind != DocKind::Invoice && !input.advances.is_empty() {
        problems.push("only an invoice deducts advances".into());
    }
    for date in [&input.due_date, &input.tax_point_date]
        .into_iter()
        .flatten()
    {
        if !skyla_ledger::is_iso_date(date) {
            problems.push(format!("{date:?} isn't a YYYY-MM-DD date"));
        }
    }
    problems
}

/// Creates a draft. Drafts can change freely until issued.
pub fn create_draft(conn: &Connection, input: &DraftInput) -> Result<i64, InvoicingError> {
    create_draft_as(conn, input, &uuid::Uuid::now_v7().to_string())
}

/// Creates a draft with a known stable id, for replays and imports whose
/// identities must come out the same every time.
pub fn create_draft_as(
    conn: &Connection,
    input: &DraftInput,
    uid: &str,
) -> Result<i64, InvoicingError> {
    let (kind, _) = series(conn, &input.series)?;
    let problems = validate(input, kind);
    if !problems.is_empty() {
        return Err(invalid(problems));
    }
    let currency = functional_currency(conn)?;
    atomically(conn, |tx| {
        tx.execute(
            "INSERT INTO document (uid, kind, series, due_date, tax_point_date, customer_name, customer_ico,
                                   customer_dic, customer_address, currency, note, related_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                uid,
                input.kind.as_str(),
                input.series,
                input.due_date,
                input.tax_point_date,
                input.customer.name,
                input.customer.ico,
                input.customer.dic,
                input.customer.address,
                currency.code(),
                input.note,
                input.related_id,
            ],
        )?;
        let id = tx.last_insert_rowid();
        write_lines(tx, id, input)?;
        Ok(id)
    })
}

fn write_lines(conn: &Connection, id: i64, input: &DraftInput) -> Result<(), InvoicingError> {
    conn.execute("DELETE FROM document_line WHERE document_id = ?1", [id])?;
    conn.execute("DELETE FROM document_advance WHERE document_id = ?1", [id])?;
    for (i, l) in input.lines.iter().enumerate() {
        conn.execute(
            "INSERT INTO document_line (document_id, line_no, description, quantity, unit, unit_price_minor, vat_code, account)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, i64::try_from(i + 1).unwrap_or(i64::MAX), l.description, l.quantity, l.unit, l.unit_price_minor, l.vat_code, l.account],
        )?;
    }
    for advance in &input.advances {
        conn.execute(
            "INSERT INTO document_advance (document_id, advance_tax_id) VALUES (?1, ?2)",
            params![id, advance],
        )?;
    }
    Ok(())
}

fn require_draft(conn: &Connection, id: i64) -> Result<(), InvoicingError> {
    let status: Option<String> = conn
        .query_row("SELECT status FROM document WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    match status.as_deref() {
        None => Err(InvoicingError::NotFound(id)),
        Some("issued") => Err(InvoicingError::Issued(id)),
        _ => Ok(()),
    }
}

/// Replaces a draft's content.
pub fn update_draft(conn: &Connection, id: i64, input: &DraftInput) -> Result<(), InvoicingError> {
    require_draft(conn, id)?;
    let (kind, _) = series(conn, &input.series)?;
    let problems = validate(input, kind);
    if !problems.is_empty() {
        return Err(invalid(problems));
    }
    atomically(conn, |tx| {
        tx.execute(
            "UPDATE document SET kind = ?2, series = ?3, due_date = ?4, tax_point_date = ?5, customer_name = ?6,
                 customer_ico = ?7, customer_dic = ?8, customer_address = ?9, note = ?10, related_id = ?11
             WHERE id = ?1",
            params![
                id,
                input.kind.as_str(),
                input.series,
                input.due_date,
                input.tax_point_date,
                input.customer.name,
                input.customer.ico,
                input.customer.dic,
                input.customer.address,
                input.note,
                input.related_id,
            ],
        )?;
        write_lines(tx, id, input)
    })
}

/// Deletes a draft. Issued documents are never deleted.
pub fn delete_draft(conn: &Connection, id: i64) -> Result<(), InvoicingError> {
    require_draft(conn, id)?;
    atomically(conn, |tx| {
        tx.execute("DELETE FROM document_advance WHERE document_id = ?1", [id])?;
        tx.execute("DELETE FROM document_line WHERE document_id = ?1", [id])?;
        tx.execute("DELETE FROM document WHERE id = ?1", [id])?;
        Ok(())
    })
}

/// Computes a document's totals with the pack's rates and rounding on `on`
/// (the tax point). VAT is computed per code from the summed bases; for an
/// advance tax document the lines hold amounts received, VAT included.
pub fn compute_totals(
    pack: &Pack,
    kind: DocKind,
    on: &str,
    currency: Currency,
    lines: &[LineInput],
) -> Result<(Vec<Line>, Totals), InvoicingError> {
    let rounding = pack.rounding("vat.rounding.document", on)?;
    let mut out = Vec::new();
    let mut per_code: BTreeMap<String, Money> = BTreeMap::new();
    for (i, l) in lines.iter().enumerate() {
        let quantity: Rate = l.quantity.parse().map_err(|_| {
            invalid(vec![format!(
                "line {}: quantity {:?} isn't a number",
                i + 1,
                l.quantity
            )])
        })?;
        let amount = Money::new(l.unit_price_minor, currency).mul_rate(quantity, rounding)?;
        let sum = per_code
            .entry(l.vat_code.clone())
            .or_insert(Money::zero(currency));
        *sum = sum.checked_add(amount)?;
        out.push(Line {
            line_no: i64::try_from(i + 1).unwrap_or(i64::MAX),
            input: l.clone(),
            amount,
        });
    }
    let mut recap = Vec::new();
    let (mut base, mut tax) = (Money::zero(currency), Money::zero(currency));
    for (code, amount) in per_code {
        let rate = pack.vat_rate(&code, on)?;
        let split = if kind == DocKind::AdvanceTax {
            vat::from_gross(amount, rate, rounding)?
        } else {
            vat::from_base(amount, rate, rounding)?
        };
        base = base.checked_add(split.base)?;
        tax = tax.checked_add(split.vat)?;
        recap.push(VatRecap {
            vat_code: code,
            rate: rate.to_string(),
            base: split.base,
            vat: split.vat,
        });
    }
    Ok((
        out,
        Totals {
            recap,
            base,
            vat: tax,
            gross: base.checked_add(tax)?,
        },
    ))
}

fn load_lines(conn: &Connection, id: i64) -> Result<Vec<LineInput>, InvoicingError> {
    let mut stmt = conn.prepare_cached(
        "SELECT description, quantity, unit, unit_price_minor, vat_code, account FROM document_line
         WHERE document_id = ?1 ORDER BY line_no",
    )?;
    let lines = stmt
        .query_map([id], |r| {
            Ok(LineInput {
                description: r.get(0)?,
                quantity: r.get(1)?,
                unit: r.get(2)?,
                unit_price_minor: r.get(3)?,
                vat_code: r.get(4)?,
                account: r.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(lines)
}

/// Loads a document. Issued documents return their stored totals.
pub fn get(conn: &Connection, pack: &Pack, id: i64) -> Result<Document, InvoicingError> {
    type Row = (
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let head = conn
        .query_row(
            "SELECT uid, kind, series, status, number, issue_date, tax_point_date, due_date,
                    customer_name, customer_ico, customer_dic, customer_address, currency, note,
                    related_id, entry_id, pack, supplier
             FROM document WHERE id = ?1",
            [id],
            |r| {
                let a: Row = (
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                );
                let customer = Customer {
                    name: r.get(8)?,
                    ico: r.get(9)?,
                    dic: r.get(10)?,
                    address: r.get(11)?,
                };
                let rest: (
                    String,
                    String,
                    Option<i64>,
                    Option<i64>,
                    Option<String>,
                    Option<String>,
                ) = (
                    r.get(12)?,
                    r.get(13)?,
                    r.get(14)?,
                    r.get(15)?,
                    r.get(16)?,
                    r.get(17)?,
                );
                Ok((a, customer, rest))
            },
        )
        .optional()?
        .ok_or(InvoicingError::NotFound(id))?;
    let (
        (uid, kind, series, status, number, issue_date, tax_point_date, due_date),
        customer,
        (currency, note, related_id, entry_id, pack_id, supplier_json),
    ) = head;
    let kind = DocKind::from_db(&kind)?;
    let currency_code = Currency::from_code(&currency)?;
    let inputs = load_lines(conn, id)?;
    let advances: Vec<i64> = conn
        .prepare_cached("SELECT advance_tax_id FROM document_advance WHERE document_id = ?1 ORDER BY advance_tax_id")?
        .query_map([id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let issued = status == "issued";
    let on = tax_point_date
        .clone()
        .or_else(|| issue_date.clone())
        .unwrap_or_else(|| "2026-01-01".into());
    let (lines, live) = compute_totals(pack, kind, &on, currency_code, &inputs)?;
    let totals = if issued {
        stored_totals(conn, id, currency_code)?
    } else {
        live
    };
    let supplier = match supplier_json {
        Some(json) => Some(supplier::from_json(&json)?),
        None if !issued => supplier::supplier(conn)?,
        None => None,
    };
    Ok(Document {
        id,
        uid,
        kind,
        series,
        issued,
        number,
        issue_date,
        tax_point_date,
        due_date,
        customer,
        currency,
        note,
        related_id,
        advances,
        entry_id,
        pack: pack_id,
        lines,
        totals,
        supplier,
    })
}

fn stored_totals(conn: &Connection, id: i64, currency: Currency) -> Result<Totals, InvoicingError> {
    let recap: Vec<VatRecap> = conn
        .prepare_cached("SELECT vat_code, rate, base_minor, vat_minor FROM document_total WHERE document_id = ?1 ORDER BY vat_code")?
        .query_map([id], |r| {
            Ok(VatRecap {
                vat_code: r.get(0)?,
                rate: r.get(1)?,
                base: Money::new(r.get(2)?, currency),
                vat: Money::new(r.get(3)?, currency),
            })
        })?
        .collect::<Result<_, _>>()?;
    let base = Money::sum(currency, recap.iter().map(|r| r.base))?;
    let vat = Money::sum(currency, recap.iter().map(|r| r.vat))?;
    Ok(Totals {
        recap,
        base,
        vat,
        gross: base.checked_add(vat)?,
    })
}

/// A known identity for an issued document's posting (replays, demos).
pub type IssueReplay<'a> = Replay<'a>;

/// The result of issuing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issued {
    /// The assigned number.
    pub number: String,
    /// The ledger entry (none for an advance invoice).
    pub entry_id: Option<i64>,
}

fn memo(kind: DocKind, number: &str, customer: &str, related: Option<&str>) -> String {
    match (kind, related) {
        (DocKind::Invoice, _) => format!("Faktura {number} {customer}"),
        (DocKind::CreditNote, Some(r)) => {
            format!("Opravný daňový doklad {number} k faktuře {r} {customer}")
        }
        (DocKind::AdvanceTax, Some(r)) => {
            format!("Daňový doklad k přijaté platbě {number} (záloha {r}) {customer}")
        }
        _ => format!("{} {number} {customer}", kind.as_str()),
    }
}

/// The journal lines a document posts, signed (debit positive).
fn posting_lines(
    conn: &Connection,
    pack: &Pack,
    accounts: &Accounts,
    doc: &Document,
) -> Result<Vec<NewLine>, InvoicingError> {
    let currency = Currency::from_code(&doc.currency)?;
    let mut lines = Vec::new();
    let push = |lines: &mut Vec<NewLine>, account: &str, minor: i64, vat_code: Option<&str>| {
        if minor != 0 {
            lines.push(NewLine {
                account: account.to_owned(),
                amount: Money::new(minor, currency),
                conversion: None,
                vat_code: vat_code.map(str::to_owned),
                tax_treatment: None,
                memo: String::new(),
            });
        }
    };
    match doc.kind {
        DocKind::Invoice | DocKind::CreditNote => {
            // Advances already taxed: take their base and VAT back out.
            let mut advance_gross = 0_i64;
            let mut deductions = Vec::new();
            for adv in &doc.advances {
                let a = get(conn, pack, *adv)?;
                if a.kind != DocKind::AdvanceTax || !a.issued {
                    return Err(invalid(vec![format!(
                        "document {adv} isn't an issued advance tax document"
                    )]));
                }
                for r in &a.totals.recap {
                    deductions.push((r.vat_code.clone(), r.base.minor(), r.vat.minor()));
                }
                advance_gross += a.totals.gross.minor();
            }
            push(
                &mut lines,
                &accounts.receivables,
                doc.totals.gross.minor() - advance_gross,
                None,
            );
            // Revenue per line; each code's lines sum to its recap base, since
            // VAT is computed on top of the summed bases.
            for r in &doc.totals.recap {
                for l in doc.lines.iter().filter(|l| l.input.vat_code == r.vat_code) {
                    let account = l.input.account.as_deref().unwrap_or(&accounts.revenue);
                    push(&mut lines, account, -l.amount.minor(), Some(&r.vat_code));
                }
                push(&mut lines, &accounts.vat, -r.vat.minor(), Some(&r.vat_code));
            }
            for (code, base, tax) in deductions {
                push(&mut lines, &accounts.advances_received, base, Some(&code));
                push(&mut lines, &accounts.vat, tax, Some(&code));
            }
        }
        DocKind::AdvanceTax => {
            push(
                &mut lines,
                &accounts.advances_received,
                doc.totals.gross.minor(),
                None,
            );
            for r in &doc.totals.recap {
                push(
                    &mut lines,
                    &accounts.advances_received,
                    -r.base.minor(),
                    Some(&r.vat_code),
                );
                push(&mut lines, &accounts.vat, -r.vat.minor(), Some(&r.vat_code));
            }
        }
        DocKind::Advance => {}
    }
    Ok(lines)
}

/// Issues a draft: assigns the next number in its series and year, fixes
/// its totals with the pack, and posts it to the ledger, all in one
/// transaction. From then on it is immutable.
pub fn issue(
    conn: &Connection,
    pack: &Pack,
    accounts: &Accounts,
    id: i64,
    issue_date: &str,
    replay: Option<IssueReplay<'_>>,
) -> Result<Issued, InvoicingError> {
    require_draft(conn, id)?;
    let year = date_year(issue_date)?;
    atomically(conn, |tx| {
        tx.execute(
            "UPDATE document SET issue_date = ?2 WHERE id = ?1",
            params![id, issue_date],
        )?;
        let mut doc = get(tx, pack, id)?;
        let on = doc
            .tax_point_date
            .clone()
            .unwrap_or_else(|| issue_date.to_owned());
        let mut problems = Vec::new();
        if doc.due_date.as_deref().is_some_and(|due| due < issue_date) {
            problems.push("the due date is before the issue date".to_owned());
        }
        if doc.kind == DocKind::Invoice && doc.due_date.is_none() {
            problems.push("an invoice needs a due date".to_owned());
        }
        if let Some(supplier) = &doc.supplier {
            for l in &doc.lines {
                let outside = pack.vat_code(&l.input.vat_code)?.outside_vat;
                if supplier.vat_payer && outside {
                    problems.push(format!(
                        "line {}: {} is for suppliers not registered for VAT",
                        l.line_no, l.input.vat_code
                    ));
                } else if !supplier.vat_payer && !outside {
                    problems.push(format!(
                        "line {}: the supplier isn't registered for VAT, so it can't charge {}",
                        l.line_no, l.input.vat_code
                    ));
                }
            }
        }
        if !problems.is_empty() {
            return Err(invalid(problems));
        }
        let (_, pattern) = series(tx, &doc.series)?;
        let seq: i64 = tx.query_row(
            "SELECT coalesce(max(seq), 0) + 1 FROM document WHERE series = ?1 AND year = ?2 AND status = 'issued'",
            params![doc.series, year],
            |r| r.get(0),
        )?;
        let number = format_number(&pattern, year, seq);
        doc.number = Some(number.clone());
        doc.tax_point_date = Some(on.clone());

        let related_number = match doc.related_id {
            Some(r) => {
                let related = get(tx, pack, r)?;
                let expected = match doc.kind {
                    DocKind::CreditNote => DocKind::Invoice,
                    _ => DocKind::Advance,
                };
                if related.kind != expected || !related.issued {
                    return Err(invalid(vec![format!(
                        "document {r} isn't an issued {}",
                        expected.as_str()
                    )]));
                }
                related.number
            }
            None => None,
        };

        let entry_id = if doc.kind.posts() {
            let lines = posting_lines(tx, pack, accounts, &doc)?;
            let entry = NewEntry {
                date: on.clone(),
                source_kind: SourceKind::Invoice,
                source_ref: Some(number.clone()),
                memo: memo(
                    doc.kind,
                    &number,
                    &doc.customer.name,
                    related_number.as_deref(),
                ),
                created_by: "user".into(),
                lines,
            };
            let uid = replay.map_or_else(|| uuid::Uuid::now_v7().to_string(), |r| r.uid.to_owned());
            let entry_id = create_entry_as(tx, &entry, &uid)?;
            // A credit note settles the invoice it corrects.
            if doc.kind == DocKind::CreditNote
                && let Some(invoice) = doc.related_id
                && let Some(invoice_entry) = get(tx, pack, invoice)?.entry_id
            {
                let currency = Currency::from_code(&doc.currency)?;
                link_settlement(
                    tx,
                    entry_id,
                    invoice_entry,
                    Money::new(-doc.totals.gross.minor(), currency),
                )?;
            }
            post_entry_at(tx, entry_id, None, replay.map(|r| r.posted_at))?;
            Some(entry_id)
        } else {
            None
        };

        for r in &doc.totals.recap {
            tx.execute(
                "INSERT INTO document_total (document_id, vat_code, rate, base_minor, vat_minor) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, r.vat_code, r.rate, r.base.minor(), r.vat.minor()],
            )?;
        }
        tx.execute(
            "UPDATE document SET status = 'issued', number = ?2, year = ?3, seq = ?4, tax_point_date = ?5,
                 entry_id = ?6, pack = ?7,
                 supplier = (SELECT profile FROM supplier_profile WHERE id = 1)
             WHERE id = ?1",
            params![id, number, year, seq, on, entry_id, pack.provenance()],
        )?;
        Ok(Issued { number, entry_id })
    })
}

/// Drafts a credit note against an issued invoice. With `lines` absent it
/// credits every line in full.
pub fn draft_credit_note(
    conn: &Connection,
    pack: &Pack,
    invoice_id: i64,
    series: &str,
    lines: Option<Vec<LineInput>>,
    reason: &str,
) -> Result<i64, InvoicingError> {
    let invoice = get(conn, pack, invoice_id)?;
    if invoice.kind != DocKind::Invoice || !invoice.issued {
        return Err(invalid(vec![format!(
            "document {invoice_id} isn't an issued invoice"
        )]));
    }
    let lines = lines.unwrap_or_else(|| {
        invoice
            .lines
            .iter()
            .map(|l| LineInput {
                quantity: if l.input.quantity.starts_with('-') {
                    l.input.quantity.trim_start_matches('-').to_owned()
                } else {
                    format!("-{}", l.input.quantity)
                },
                ..l.input.clone()
            })
            .collect()
    });
    create_draft(
        conn,
        &DraftInput {
            kind: DocKind::CreditNote,
            series: series.to_owned(),
            customer: invoice.customer.clone(),
            due_date: None,
            tax_point_date: None,
            note: reason.to_owned(),
            lines,
            related_id: Some(invoice_id),
            advances: Vec::new(),
        },
    )
}

/// What's paid, credited and open on an issued document.
pub fn state(conn: &Connection, pack: &Pack, id: i64) -> Result<DocState, InvoicingError> {
    let doc = get(conn, pack, id)?;
    let currency = Currency::from_code(&doc.currency)?;
    let zero = Money::zero(currency);
    let mut advances = zero;
    for adv in &doc.advances {
        advances = advances.checked_add(get(conn, pack, *adv)?.totals.gross)?;
    }
    let Some(entry_id) = doc.entry_id.filter(|_| doc.issued) else {
        return Ok(DocState {
            gross: doc.totals.gross,
            advances,
            paid: zero,
            credited: zero,
            open: if doc.issued { zero } else { doc.totals.gross },
            settlement: if doc.issued {
                Settlement::Open
            } else {
                Settlement::Draft
            },
            settled_on: None,
        });
    };
    let (mut paid, mut credited) = (zero, zero);
    let mut last = None;
    for link in settlements_of(conn, entry_id)? {
        let is_credit: bool = conn.query_row(
            "SELECT count(*) > 0 FROM document WHERE entry_id = ?1 AND kind = 'credit_note'",
            [link.cash_entry_id],
            |r| r.get(0),
        )?;
        if is_credit {
            credited = credited.checked_add(link.amount)?;
        } else {
            paid = paid.checked_add(link.amount)?;
        }
        last = Some(link.date);
    }
    let open = doc
        .totals
        .gross
        .checked_sub(advances)?
        .checked_sub(paid)?
        .checked_sub(credited)?;
    let settlement = if !open.is_zero() && open.is_negative() {
        Settlement::Paid
    } else if open.is_zero() {
        if paid.is_zero() && !credited.is_zero() {
            Settlement::Credited
        } else {
            Settlement::Paid
        }
    } else if paid.is_zero() && credited.is_zero() {
        Settlement::Open
    } else {
        Settlement::PartlySettled
    };
    Ok(DocState {
        gross: doc.totals.gross,
        advances,
        paid,
        credited,
        open,
        settlement,
        settled_on: if open.is_zero() { last } else { None },
    })
}

/// What reduced an issued document's debt, by date: payments and credit
/// notes, each settling part of it (a reversed payment counts negative).
pub fn reductions(
    conn: &Connection,
    pack: &Pack,
    id: i64,
) -> Result<Vec<(String, Money)>, InvoicingError> {
    let doc = get(conn, pack, id)?;
    let Some(entry_id) = doc.entry_id.filter(|_| doc.issued) else {
        return Ok(Vec::new());
    };
    Ok(settlements_of(conn, entry_id)?
        .into_iter()
        .map(|l| (l.date, l.amount))
        .collect())
}

/// Numbers missing from a series' issued sequence, per year. Issued numbers
/// are gapless by construction; this checks the stored books (imports, raw edits).
pub fn series_gaps(conn: &Connection, code: &str) -> Result<Vec<String>, InvoicingError> {
    let (_, pattern) = series(conn, code)?;
    let rows: Vec<(i64, i64)> = conn
        .prepare("SELECT year, seq FROM document WHERE series = ?1 AND status = 'issued' ORDER BY year, seq")?
        .query_map([code], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut gaps = Vec::new();
    let mut by_year: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for (y, s) in rows {
        by_year.entry(y).or_default().push(s);
    }
    for (year, seqs) in by_year {
        let mut expected = 1;
        for s in seqs {
            while expected < s {
                gaps.push(format_number(&pattern, year, expected));
                expected += 1;
            }
            expected = s + 1;
        }
    }
    Ok(gaps)
}

/// The namespace of imported documents' ids (UUID v5 of `entry uid/number`).
const IMPORT_NAMESPACE: uuid::Uuid = uuid::uuid!("6b1e2c9a-3f0d-5e7b-9a41-0c8d2f5e7a13");

/// Records a document that was issued and posted elsewhere (an import from
/// another program, or the demo's golden journal). The ledger entry must
/// already exist and carry exactly the document's totals. The document's id
/// derives from the entry's, so importing the same books twice yields the
/// same documents.
pub fn import_issued(
    conn: &Connection,
    pack: &Pack,
    accounts: &Accounts,
    input: &DraftInput,
    number: &str,
    issue_date: &str,
    entry_id: i64,
) -> Result<i64, InvoicingError> {
    let (kind, pattern) = series(conn, &input.series)?;
    let problems = validate(input, kind);
    if !problems.is_empty() {
        return Err(invalid(problems));
    }
    let year = date_year(issue_date)?;
    let seq = (1..100_000)
        .find(|s| format_number(&pattern, year, *s) == number)
        .ok_or_else(|| {
            invalid(vec![format!(
                "{number} doesn't fit series {} ({pattern})",
                input.series
            )])
        })?;
    let entry = get_entry(conn, entry_id)?;
    let currency = functional_currency(conn)?;
    let on = input
        .tax_point_date
        .clone()
        .unwrap_or_else(|| entry.date.clone());
    let (_, totals) = compute_totals(pack, kind, &on, currency, &input.lines)?;
    let receivable: i64 = entry
        .lines
        .iter()
        .filter(|l| l.account == accounts.receivables)
        .map(|l| l.functional.minor())
        .sum();
    if receivable != totals.gross.minor() {
        return Err(invalid(vec![format!(
            "entry {entry_id} puts {receivable} on receivables; document {number} totals {}",
            totals.gross.minor()
        )]));
    }
    let uid = uuid::Uuid::new_v5(
        &IMPORT_NAMESPACE,
        format!("{}/{number}", entry.uid).as_bytes(),
    );
    atomically(conn, |tx| {
        let id = create_draft_as(tx, input, &uid.to_string())?;
        for r in &totals.recap {
            tx.execute(
                "INSERT INTO document_total (document_id, vat_code, rate, base_minor, vat_minor) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, r.vat_code, r.rate, r.base.minor(), r.vat.minor()],
            )?;
        }
        tx.execute(
            "UPDATE document SET status = 'issued', number = ?2, year = ?3, seq = ?4, issue_date = ?5,
                 tax_point_date = ?6, entry_id = ?7, pack = ?8, imported = 1,
                 supplier = (SELECT profile FROM supplier_profile WHERE id = 1)
             WHERE id = ?1",
            params![id, number, year, seq, issue_date, on, entry_id, pack.provenance()],
        )?;
        Ok(id)
    })
}
