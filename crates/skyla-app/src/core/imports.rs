//! Moving in from Pohoda or Fakturoid (WP-31): issued invoices and credit
//! notes read from the other program's export, previewed, and on the user's
//! go posted through the kernel and recorded as imported documents. Each
//! document is new, already in these books, or has a problem the preview
//! names; only new ones are posted. Payments aren't imported: the bank
//! statements and the matcher settle them as for any invoice.
//!
//! A credit note is new only against an invoice that is, or will be, in the
//! books: originals come first. The preview resolves every invoice of the
//! file before any credit note, whatever their order in the file, and the
//! commit posts all new invoices before all new credit notes (each group in
//! file order). A credit note is checked against what is still open on its
//! original (gross less payments and earlier credits, the in-file ones
//! included), the same cap the ledger enforces when the settlement is
//! linked, and posts as the reverse of an invoice: Dr revenue, Dr VAT, Cr
//! receivables, linked as a settlement of the invoice's entry.

use skyla_invoicing::import::{self, Band, ImportFile, Imported};
use skyla_invoicing::{Accounts, DocKind, DraftInput, LineInput};
use skyla_ledger::{
    NewEntry, NewLine, PeriodState, SourceKind, create_draft_as, find_posted_by_ref,
    link_settlement, list_periods, post_entry_at,
};
use skyla_money::Money;

use super::{Core, money};
use crate::dto::{ImportDocumentDto, ImportPreviewDto};
use crate::error::CoreError;

/// The namespace of imported entries' ids (UUID v5 of `source/number`).
const IMPORT_NAMESPACE: uuid::Uuid = uuid::uuid!("2f6a9c41-7d3e-5b18-8e02-4c9d1a7f3b65");

fn bad(msg: impl Into<String>) -> CoreError {
    CoreError::BadRequest(msg.into())
}

/// A credit note's parts are negative, and the kernel issues those as a
/// quantity of -1 at the positive base; an invoice's are 1 at the base.
fn quantity_of(part: &import::Part) -> &'static str {
    if part.base_minor < 0 || (part.base_minor == 0 && part.vat_minor < 0) {
        "-1"
    } else {
        "1"
    }
}

/// `-12 100,50 CZK` for messages.
fn show(minor: i64, code: &str) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    format!("{sign}{},{:02} {code}", abs / 100, abs % 100)
}

/// A series pattern for a number: the issue year becomes `{YYYY}` (or
/// `{YY}`) and the trailing digits the counter. `2026-044` → `{YYYY}-{NNN}`.
fn pattern_of(number: &str, year: &str) -> Option<String> {
    let digits = number.len() - number.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return None;
    }
    let (head, _) = number.split_at(number.len() - digits);
    let head = if head.contains(year) {
        head.replacen(year, "{YYYY}", 1)
    } else if let Some(yy) = year.get(2..)
        && head.contains(yy)
        && !head.ends_with(yy)
    {
        head.replacen(yy, "{YY}", 1)
    } else if head.is_empty() && number.starts_with(year) && digits > 4 {
        // `2026044`: the year glued to the counter.
        return Some(format!("{{YYYY}}{{{}}}", "N".repeat(digits - 4)));
    } else {
        head.to_owned()
    };
    Some(format!("{head}{{{}}}", "N".repeat(digits)))
}

/// What a document would post, or why it can't.
struct Plan {
    input: DraftInput,
    date: String,
    series: Result<String, String>,
    /// A credit note's invoice, by number.
    original: Option<String>,
}

/// An invoice a credit note can correct, as far as this import has used it.
#[derive(Clone)]
struct Original {
    date: String,
    ico: Option<String>,
    /// Still open (not paid, not credited), in minor units.
    open: i64,
}

/// What a credit note is checked against.
enum Against<'a> {
    /// The document is an invoice.
    Nothing,
    /// Its invoice.
    Found(&'a str, &'a Original),
    /// Why its invoice can't be used.
    Missing(String),
}

/// How the preview judged one document.
struct Verdict {
    status: &'static str,
    problems: Vec<String>,
    plan: Option<Plan>,
}

impl Verdict {
    fn duplicate(why: &str) -> Self {
        Self {
            status: "duplicate",
            problems: vec![why.to_owned()],
            plan: None,
        }
    }
}

impl Core {
    fn read_import(content_base64: &str) -> Result<ImportFile, CoreError> {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(content_base64.trim())
            .map_err(|_| bad("the file isn't base64"))?;
        import::parse(&bytes)
            .ok_or_else(|| bad("that isn't a Pohoda XML export or a Fakturoid CSV export"))
    }

    /// The VAT code for a part, or why there's none.
    fn import_code(&self, kind: DocKind, part: &import::Part, on: &str) -> Result<String, String> {
        let vat_payer = self.domain.supplier.vat_payer;
        if !vat_payer {
            return if part.vat_minor == 0 {
                Ok("NOVAT".into())
            } else {
                Err("charges VAT, but these books aren't registered for VAT".into())
            };
        }
        let candidates: &[&str] = match part.band {
            Band::Standard => &["OUT21"],
            Band::Reduced => &["OUT12"],
            Band::Unstated => &["OUT21", "OUT12"],
            Band::None => {
                return Err(
                    "has a supply without VAT; sky-la can't tell which exemption, so enter it by hand"
                        .into(),
                );
            }
        };
        let currency = self.currency;
        for code in candidates {
            let line = LineInput {
                description: String::new(),
                quantity: quantity_of(part).into(),
                unit: String::new(),
                unit_price_minor: part.base_minor.abs(),
                vat_code: (*code).to_owned(),
                account: None,
            };
            if let Ok((_, totals)) =
                skyla_invoicing::compute_totals(&self.pack, kind, on, currency, &[line])
                && totals.vat.minor() == part.vat_minor
            {
                return Ok((*code).to_owned());
            }
        }
        Err(match part.band {
            Band::Unstated => "VAT doesn't match any rate in the rule pack".into(),
            _ => "VAT doesn't match the rate on its tax point; check the original".into(),
        })
    }

    /// Checks one document; its problems, or what it would post. A credit
    /// note is checked against `against`, the invoice it corrects.
    fn plan_import(
        &self,
        source: import::Source,
        d: &Imported,
        against: &Against<'_>,
    ) -> Result<Plan, Vec<String>> {
        let credit = d.kind == DocKind::CreditNote;
        let code = self.currency.code();
        let mut problems = Vec::new();
        if d.currency != code {
            problems.push(format!(
                "is in {}; only {code} documents are imported",
                d.currency
            ));
        }
        if let Some(ico) = &d.customer.ico
            && !skyla_invoicing::valid_ico(ico)
        {
            problems.push(format!("the customer's IČO {ico} isn't valid"));
        }
        let date = d
            .tax_point_date
            .clone()
            .unwrap_or_else(|| d.issue_date.clone());
        match list_periods(&self.db()) {
            Ok(periods) => match periods.iter().find(|p| {
                p.starts_on.as_str() <= date.as_str() && p.ends_on.as_str() >= date.as_str()
            }) {
                None => problems.push(format!("{date} is outside the periods these books keep")),
                Some(p) if p.state == PeriodState::Closed => {
                    problems.push(format!("{date} is in a closed period"));
                }
                Some(_) => {}
            },
            Err(e) => problems.push(e.to_string()),
        }
        if credit {
            match against {
                Against::Missing(why) => problems.push(why.clone()),
                Against::Found(number, original) => {
                    if d.total_minor >= 0 {
                        problems.push("credits nothing".into());
                    } else if d.total_minor.saturating_neg() > original.open {
                        problems.push(format!(
                            "credits {} but only {} of invoice {number} is still open (the rest is paid or already credited)",
                            show(d.total_minor.saturating_neg(), code),
                            show(original.open, code),
                        ));
                    }
                    if date < original.date {
                        problems.push(format!(
                            "is dated {date}, before the invoice {number} it corrects ({})",
                            original.date
                        ));
                    }
                    if let (Some(a), Some(b)) = (&d.customer.ico, &original.ico)
                        && a != b
                    {
                        problems.push(format!("is for another customer than invoice {number}"));
                    }
                }
                Against::Nothing => {}
            }
        } else if d.total_minor < 0 || d.parts.iter().any(|p| p.base_minor < 0 || p.vat_minor < 0) {
            problems.push("has negative amounts; import it as a credit note instead".into());
        }
        let what = if credit {
            "Opravný daňový doklad"
        } else {
            "Faktura"
        };
        let mut lines = Vec::new();
        for part in &d.parts {
            match self.import_code(d.kind, part, &date) {
                Ok(vat_code) => lines.push(LineInput {
                    description: d
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("{what} {} ({})", d.number, source.name())),
                    quantity: quantity_of(part).into(),
                    unit: String::new(),
                    unit_price_minor: part.base_minor.abs(),
                    vat_code,
                    account: None,
                }),
                Err(p) => problems.push(p),
            }
        }
        let stated: i64 = d.parts.iter().map(|p| p.base_minor + p.vat_minor).sum();
        if stated != d.total_minor {
            problems.push("its total isn't the sum of its bases and VAT".into());
        }
        if !problems.is_empty() {
            return Err(problems);
        }
        let series = self.import_series(d);
        Ok(Plan {
            input: DraftInput {
                kind: d.kind,
                series: series.clone().unwrap_or_default(),
                customer: d.customer.clone(),
                // A credit note asks for no payment.
                due_date: if credit { None } else { d.due_date.clone() },
                tax_point_date: Some(date.clone()),
                note: format!("Imported from {}.", source.name()),
                lines,
                related_id: None,
                advances: Vec::new(),
            },
            date,
            series,
            original: if credit {
                d.original_number.clone()
            } else {
                None
            },
        })
    }

    /// The series an imported number belongs to: an existing series of the
    /// document's kind with the same pattern, or `Err(pattern)` for a new one.
    fn import_series(&self, d: &Imported) -> Result<String, String> {
        let year = d.issue_date.get(..4).unwrap_or_default();
        let pattern = pattern_of(&d.number, year).unwrap_or_else(|| d.number.clone());
        let found: Option<String> = self
            .db()
            .query_row(
                "SELECT code FROM doc_series WHERE kind = ?1 AND pattern = ?2 ORDER BY code LIMIT 1",
                [d.kind.as_str(), &pattern],
                |r| r.get(0),
            )
            .ok();
        found.ok_or(pattern)
    }

    fn already_here(&self, number: &str) -> bool {
        let db = self.db();
        let doc: bool = db
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM document WHERE number = ?1)",
                [number],
                |r| r.get(0),
            )
            .unwrap_or(false);
        doc || find_posted_by_ref(&db, SourceKind::Invoice, number)
            .ok()
            .flatten()
            .is_some()
    }

    /// The id of the issued invoice with this number, if the books have one.
    fn invoice_id(db: &rusqlite::Connection, number: &str) -> Option<i64> {
        db.query_row(
            "SELECT id FROM document WHERE number = ?1 AND kind = 'invoice' AND status = 'issued' ORDER BY id LIMIT 1",
            [number],
            |r| r.get(0),
        )
        .ok()
    }

    /// An issued invoice in the books, with what is still open on it.
    fn book_original(&self, number: &str) -> Option<Original> {
        let db = self.db();
        let id = Self::invoice_id(&db, number)?;
        let doc = skyla_invoicing::get(&db, &self.pack, id).ok()?;
        let state = skyla_invoicing::state(&db, &self.pack, id).ok()?;
        Some(Original {
            date: doc.tax_point_date.or(doc.issue_date).unwrap_or_default(),
            ico: doc.customer.ico,
            open: state.open.minor(),
        })
    }

    /// Judges every document of the file. Invoices first, then credit notes
    /// (whatever their order in the file), so a credit note can correct an
    /// invoice that arrives in the same file and each one lowers what is
    /// left open on its original for the next.
    fn evaluate(&self, file: &ImportFile) -> Vec<Verdict> {
        let mut seen = std::collections::HashSet::new();
        let mut verdicts: Vec<Option<Verdict>> = file
            .documents
            .iter()
            .map(|d| {
                if self.already_here(&d.number) {
                    Some(Verdict::duplicate("already in these books"))
                } else if !seen.insert(d.number.clone()) {
                    Some(Verdict::duplicate("appears twice in the file"))
                } else {
                    None
                }
            })
            .collect();
        let mut originals: std::collections::HashMap<String, Original> =
            std::collections::HashMap::new();
        for credit_notes in [false, true] {
            for (d, slot) in file.documents.iter().zip(verdicts.iter_mut()) {
                if slot.is_some() || (d.kind == DocKind::CreditNote) != credit_notes {
                    continue;
                }
                let original = d.original_number.clone().unwrap_or_default();
                let planned = if credit_notes {
                    if !originals.contains_key(&original)
                        && let Some(found) = self.book_original(&original)
                    {
                        originals.insert(original.clone(), found);
                    }
                    let against = match originals.get(&original) {
                        Some(o) => Against::Found(&original, o),
                        None => Against::Missing(missing_original(file, d)),
                    };
                    self.plan_import(file.source, d, &against)
                } else {
                    self.plan_import(file.source, d, &Against::Nothing)
                };
                *slot = Some(match planned {
                    Ok(plan) => {
                        if credit_notes {
                            if let Some(o) = originals.get_mut(&original) {
                                o.open += d.total_minor;
                            }
                        } else {
                            originals.insert(
                                d.number.clone(),
                                Original {
                                    date: plan.date.clone(),
                                    ico: d.customer.ico.clone(),
                                    open: d.total_minor,
                                },
                            );
                        }
                        Verdict {
                            status: "new",
                            problems: Vec::new(),
                            plan: Some(plan),
                        }
                    }
                    Err(problems) => Verdict {
                        status: "problem",
                        problems,
                        plan: None,
                    },
                });
            }
        }
        verdicts
            .into_iter()
            .map(|v| v.unwrap_or_else(|| Verdict::duplicate("not judged")))
            .collect()
    }

    fn preview_dto(
        &self,
        file_name: &str,
        file: &ImportFile,
        verdicts: Vec<Verdict>,
    ) -> Result<ImportPreviewDto, CoreError> {
        let mut documents = Vec::new();
        for (d, v) in file.documents.iter().zip(verdicts) {
            documents.push(ImportDocumentDto {
                position: u32::try_from(d.position).unwrap_or(u32::MAX),
                number: d.number.clone(),
                kind: d.kind.as_str().into(),
                issue_date: d.issue_date.clone(),
                customer: d.customer.name.clone(),
                base: money(Money::new(
                    d.parts.iter().map(|p| p.base_minor).sum(),
                    self.currency,
                ))?,
                vat: money(Money::new(
                    d.parts.iter().map(|p| p.vat_minor).sum(),
                    self.currency,
                ))?,
                total: money(Money::new(d.total_minor, self.currency))?,
                status: v.status.into(),
                problems: v.problems,
            });
        }
        let new = documents.iter().filter(|d| d.status == "new").count();
        Ok(ImportPreviewDto {
            file: file_name.to_owned(),
            source: file.source.name().into(),
            new: u32::try_from(new).unwrap_or(u32::MAX),
            documents,
            problems: file.problems.clone(),
            imported: Vec::new(),
        })
    }

    fn preview_of(
        &self,
        file_name: &str,
        file: &ImportFile,
    ) -> Result<ImportPreviewDto, CoreError> {
        let verdicts = self.evaluate(file);
        self.preview_dto(file_name, file, verdicts)
    }

    /// What importing a file would do. Changes nothing.
    pub fn preview_invoice_import(
        &self,
        file_name: &str,
        content_base64: &str,
    ) -> Result<ImportPreviewDto, CoreError> {
        let file = Self::read_import(content_base64)?;
        self.preview_of(file_name, &file)
    }

    /// Posts every new document in the file, each as one entry approved by
    /// the user, and records it as an issued document: the invoices first,
    /// then the credit notes against them. All or nothing.
    pub fn commit_invoice_import(
        &self,
        file_name: &str,
        content_base64: &str,
    ) -> Result<ImportPreviewDto, CoreError> {
        let file = Self::read_import(content_base64)?;
        // Plans first: making them reads the books, so before the lock.
        let verdicts = self.evaluate(&file);
        let mut plans: Vec<(&Imported, Plan)> = file
            .documents
            .iter()
            .zip(verdicts)
            .filter_map(|(d, v)| v.plan.map(|plan| (d, plan)))
            .collect();
        if plans.is_empty() {
            return Err(bad("there's nothing new to import in this file"));
        }
        // Stable: file order within invoices and within credit notes.
        plans.sort_by_key(|(d, _)| d.kind == DocKind::CreditNote);
        let posted_at = format!("{}T12:00:00.000Z", self.domain.entity.as_of);
        let mut imported = Vec::new();
        {
            let db = self.db();
            db.execute_batch("SAVEPOINT invoice_import")
                .map_err(|e| bad(e.to_string()))?;
            let mut created = std::collections::HashMap::new();
            let result = (|| -> Result<(), CoreError> {
                for (d, plan) in plans {
                    self.post_import(&db, file.source, d, plan, &posted_at, &mut created)?;
                    imported.push(d.number.clone());
                }
                Ok(())
            })();
            if let Err(e) = result {
                let _ = db.execute_batch("ROLLBACK TO invoice_import; RELEASE invoice_import");
                return Err(e);
            }
            db.execute_batch("RELEASE invoice_import")
                .map_err(|e| bad(e.to_string()))?;
        }
        let mut after = self.preview_of(file_name, &file)?;
        after.imported = imported;
        Ok(after)
    }

    /// Posts one planned document and records it. `created` holds the
    /// series this import defined, by kind and pattern.
    fn post_import(
        &self,
        db: &rusqlite::Connection,
        source: import::Source,
        d: &Imported,
        mut plan: Plan,
        posted_at: &str,
        created: &mut std::collections::HashMap<String, String>,
    ) -> Result<(), CoreError> {
        let accounts = Accounts::cz();
        plan.input.series = match plan.series {
            Ok(code) => code,
            Err(pattern) => {
                let key = format!("{}:{pattern}", d.kind.as_str());
                if let Some(code) = created.get(&key) {
                    code.clone()
                } else {
                    let n: i64 = db
                        .query_row(
                            "SELECT count(*) FROM doc_series WHERE code LIKE 'IMP%'",
                            [],
                            |r| r.get(0),
                        )
                        .map_err(|e| bad(e.to_string()))?;
                    let code = format!("IMP{}", n + 1);
                    skyla_invoicing::define_series(
                        db,
                        &code,
                        d.kind,
                        &pattern,
                        &format!("Imported from {}", source.name()),
                    )?;
                    created.insert(key, code.clone());
                    code
                }
            }
        };
        // A credit note corrects an invoice the books have by now: one that
        // was there, or one this import posted just before.
        let original = match &plan.original {
            Some(number) => {
                let id = Self::invoice_id(db, number).ok_or_else(|| {
                    bad(format!("{}: invoice {number} isn't in the books", d.number))
                })?;
                let entry = skyla_invoicing::get(db, &self.pack, id)?.entry_id;
                plan.input.related_id = Some(id);
                Some((number.clone(), entry))
            }
            None => None,
        };
        let (_, totals) = skyla_invoicing::compute_totals(
            &self.pack,
            d.kind,
            &plan.date,
            self.currency,
            &plan.input.lines,
        )?;
        // The same lines for both kinds, signed: a credit note's amounts
        // are negative, so it posts Dr revenue / Dr VAT / Cr receivables.
        let mut lines = vec![NewLine::debit(&accounts.receivables, totals.gross)];
        for r in &totals.recap {
            let mut revenue = NewLine::credit(&accounts.revenue, r.base)?;
            revenue.vat_code = Some(r.vat_code.clone());
            lines.push(revenue);
            if !r.vat.is_zero() {
                let mut vat = NewLine::credit(&accounts.vat, r.vat)?;
                vat.vat_code = Some(r.vat_code.clone());
                lines.push(vat);
            }
        }
        let memo = match &original {
            Some((number, _)) => format!(
                "Opravný daňový doklad {} k faktuře {number} {}",
                d.number, d.customer.name
            ),
            None => format!("Faktura {} {}", d.number, d.customer.name),
        };
        let entry = NewEntry {
            date: plan.date.clone(),
            source_kind: SourceKind::Invoice,
            source_ref: Some(d.number.clone()),
            memo,
            created_by: "user".into(),
            lines,
        };
        let uid = uuid::Uuid::new_v5(
            &IMPORT_NAMESPACE,
            format!("{}/{}", source.name(), d.number).as_bytes(),
        );
        let entry_id = create_draft_as(db, &entry, &uid.to_string())?;
        // A credit note settles the invoice it corrects (the ledger refuses
        // more than the invoice's gross, payments and credits together).
        if let Some((_, Some(invoice_entry))) = original {
            link_settlement(db, entry_id, invoice_entry, totals.gross.checked_neg()?)?;
        }
        post_entry_at(db, entry_id, Some("user"), Some(posted_at))?;
        skyla_invoicing::import_issued(
            db,
            &self.pack,
            &accounts,
            &plan.input,
            &d.number,
            &d.issue_date,
            entry_id,
        )?;
        Ok(())
    }
}

/// Why a credit note's invoice can't be used.
fn missing_original(file: &ImportFile, d: &Imported) -> String {
    match d.original_number.as_deref() {
        None => "names no original invoice, so it can't be matched to one; add the invoice number to the export".into(),
        Some(n) if file.documents.iter().any(|o| o.kind == DocKind::Invoice && o.number == n) => {
            format!("the invoice {n} it corrects is in this file but won't import, so neither can this credit note")
        }
        Some(n) => format!(
            "the invoice {n} it corrects isn't among the issued invoices in these books or in this file; import the invoice first"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::pattern_of;
    use crate::Core;
    use base64::Engine as _;

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    const POHODA_CREDIT: &[u8] =
        include_bytes!("../../../../packages/fixtures/data/imports/pohoda-dobropisy.xml");

    /// What the books hold for an imported credit note: its series, the
    /// invoice it points at, and its entry's status and approver.
    fn record(core: &Core, number: &str) -> (String, String, String, String, bool) {
        core.db()
            .query_row(
                "SELECT d.series, d.kind, o.number, e.status || '/' || e.approved_by, d.imported
                 FROM document d
                 JOIN document o ON o.id = d.related_id
                 JOIN journal_entry e ON e.id = d.entry_id
                 WHERE d.number = ?1",
                [number],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap()
    }

    #[test]
    fn imported_credit_notes_are_documents_linked_to_their_invoice_in_the_matching_series() {
        let core = Core::demo().unwrap();
        core.commit_invoice_import("p.xml", &b64(POHODA_CREDIT))
            .unwrap();
        // OD2026-001 fits the demo's credit-note series OD (OD{YYYY}-{NNN}).
        assert_eq!(
            record(&core, "OD2026-001"),
            (
                "OD".into(),
                "credit_note".into(),
                "2026-102".into(),
                "posted/user".into(),
                true
            )
        );
        // DB26-001 fits none: a new credit-note series, IMP1 (the invoices of
        // this file fit FV, so no invoice series is added).
        let (series, kind, original, ..) = record(&core, "DB26-001");
        assert_eq!(
            (series.as_str(), kind.as_str(), original.as_str()),
            ("IMP1", "credit_note", "2026-130")
        );
        let (pattern, count): (String, i64) = core
            .db()
            .query_row(
                "SELECT pattern, (SELECT count(*) FROM doc_series WHERE code LIKE 'IMP%')
                 FROM doc_series WHERE code = 'IMP1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((pattern.as_str(), count), ("DB{YY}-{NNN}", 1));
        // The settlement link carries the credit's size, 12 100,00, to the
        // invoice's own entry.
        let amount: i64 = core
            .db()
            .query_row(
                "SELECT s.amount_func_minor FROM settlement s
                 JOIN document c ON c.entry_id = s.cash_entry_id
                 JOIN document o ON o.entry_id = s.settled_entry_id
                 WHERE c.number = 'OD2026-001' AND o.number = '2026-102'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(amount, 1_210_000);
        // Dr 602 / Dr 343 / Cr 311: the reverse of the invoice posting.
        let lines: Vec<(String, i64)> = {
            let db = core.db();
            let mut stmt = db
                .prepare(
                    "SELECT a.code, p.amount_func_minor FROM posting p
                     JOIN account a ON a.id = p.account_id
                     JOIN document d ON d.entry_id = p.entry_id
                     WHERE d.number = 'OD2026-001' ORDER BY p.line_no",
                )
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(
            lines,
            [
                ("311".to_owned(), -1_210_000),
                ("602".to_owned(), 1_000_000),
                ("343".to_owned(), 210_000)
            ]
        );
    }

    #[test]
    fn a_credit_note_in_a_closed_period_is_refused() {
        let core = Core::demo().unwrap();
        {
            let db = core.db();
            // Q2 and Q3, in order (the ledger closes earlier periods first).
            for starts_on in ["2026-04-01", "2026-07-01"] {
                let period = skyla_ledger::list_periods(&db)
                    .unwrap()
                    .into_iter()
                    .find(|p| p.starts_on == starts_on)
                    .unwrap();
                skyla_ledger::begin_close(&db, period.id).unwrap();
                skyla_ledger::close_period(&db, period.id, &[], "test").unwrap();
            }
        }
        // 1 000,00 + 210,00 against 2026-102 (issued 2026-08-25), dated in Q3.
        let csv = "Číslo;Vystaveno;Odběratel;IČO;Původní doklad;Bez DPH;DPH;Celkem\n\
                   OD2026-030;10.09.2026;Studio Brno s.r.o.;94722188;2026-102;-1 000,00;-210,00;-1 210,00\n";
        let p = core
            .preview_invoice_import("x.csv", &b64(csv.as_bytes()))
            .unwrap();
        assert_eq!(p.documents[0].status, "problem");
        assert_eq!(
            p.documents[0].problems,
            ["2026-09-10 is in a closed period"]
        );
    }

    #[test]
    fn series_patterns_from_numbers() {
        assert_eq!(
            pattern_of("2026-044", "2026").as_deref(),
            Some("{YYYY}-{NNN}")
        );
        assert_eq!(
            pattern_of("FA-2026-0007", "2026").as_deref(),
            Some("FA-{YYYY}-{NNNN}")
        );
        assert_eq!(
            pattern_of("26FV012", "2026").as_deref(),
            Some("{YY}FV{NNN}")
        );
        assert_eq!(
            pattern_of("2026044", "2026").as_deref(),
            Some("{YYYY}{NNN}")
        );
        assert_eq!(pattern_of("V-17", "2026").as_deref(), Some("V-{NN}"));
        assert_eq!(pattern_of("ABC", "2026"), None);
    }
}
