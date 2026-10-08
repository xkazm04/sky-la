//! Moving in from Pohoda or Fakturoid (WP-31): issued invoices read from
//! the other program's export, previewed, and on the user's go posted
//! through the kernel and recorded as imported documents. Each document is
//! new, already in these books, or has a problem the preview names; only
//! new ones are posted. Payments aren't imported: the bank statements and
//! the matcher settle them as for any invoice.

use skyla_invoicing::import::{self, Band, ImportFile, Imported};
use skyla_invoicing::{Accounts, DocKind, DraftInput, LineInput};
use skyla_ledger::{
    NewEntry, NewLine, PeriodState, SourceKind, create_draft_as, find_posted_by_ref, list_periods,
    post_entry_at,
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
    fn import_code(&self, part: &import::Part, on: &str) -> Result<String, String> {
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
                quantity: "1".into(),
                unit: String::new(),
                unit_price_minor: part.base_minor,
                vat_code: (*code).to_owned(),
                account: None,
            };
            if let Ok((_, totals)) =
                skyla_invoicing::compute_totals(&self.pack, DocKind::Invoice, on, currency, &[line])
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

    /// Checks one document; its problems, or what it would post.
    fn plan_import(&self, source: import::Source, d: &Imported) -> Result<Plan, Vec<String>> {
        let mut problems = Vec::new();
        if d.kind != DocKind::Invoice {
            problems.push("credit notes aren't imported yet; enter it against its invoice".into());
        }
        if d.currency != self.currency.code() {
            problems.push(format!(
                "is in {}; only {} documents are imported",
                d.currency,
                self.currency.code()
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
        let mut lines = Vec::new();
        for part in &d.parts {
            match self.import_code(part, &date) {
                Ok(code) => lines.push(LineInput {
                    description: d
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("Faktura {} ({})", d.number, source.name())),
                    quantity: "1".into(),
                    unit: String::new(),
                    unit_price_minor: part.base_minor,
                    vat_code: code,
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
                kind: DocKind::Invoice,
                series: series.clone().unwrap_or_default(),
                customer: d.customer.clone(),
                due_date: d.due_date.clone(),
                tax_point_date: Some(date.clone()),
                note: format!("Imported from {}.", source.name()),
                lines,
                related_id: None,
                advances: Vec::new(),
            },
            date,
            series,
        })
    }

    /// The series an imported number belongs to: an existing invoice series
    /// with the same pattern, or `Err(pattern)` for a new one.
    fn import_series(&self, d: &Imported) -> Result<String, String> {
        let year = d.issue_date.get(..4).unwrap_or_default();
        let pattern = pattern_of(&d.number, year).unwrap_or_else(|| d.number.clone());
        let found: Option<String> = self
            .db()
            .query_row(
                "SELECT code FROM doc_series WHERE kind = 'invoice' AND pattern = ?1 ORDER BY code LIMIT 1",
                [&pattern],
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

    fn preview_of(
        &self,
        file_name: &str,
        file: &ImportFile,
    ) -> Result<ImportPreviewDto, CoreError> {
        let mut seen = std::collections::HashSet::new();
        let mut documents = Vec::new();
        for d in &file.documents {
            let (status, problems) = if self.already_here(&d.number) {
                ("duplicate", vec!["already in these books".to_owned()])
            } else if !seen.insert(d.number.clone()) {
                ("duplicate", vec!["appears twice in the file".to_owned()])
            } else {
                match self.plan_import(file.source, d) {
                    Ok(_) => ("new", Vec::new()),
                    Err(p) => ("problem", p),
                }
            };
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
                status: status.into(),
                problems,
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
    /// the user, and records it as an issued document. All or nothing.
    pub fn commit_invoice_import(
        &self,
        file_name: &str,
        content_base64: &str,
    ) -> Result<ImportPreviewDto, CoreError> {
        let file = Self::read_import(content_base64)?;
        let preview = self.preview_of(file_name, &file)?;
        if preview.new == 0 {
            return Err(bad("there's nothing new to import in this file"));
        }
        // Plans first: making them reads the books, so before the lock.
        let mut plans = Vec::new();
        for (d, p) in file.documents.iter().zip(&preview.documents) {
            if p.status == "new" {
                let plan = self
                    .plan_import(file.source, d)
                    .map_err(|p| bad(format!("{}: {}", d.number, p.join("; "))))?;
                plans.push((d, plan));
            }
        }
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
    /// series this import defined, by pattern.
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
                if let Some(code) = created.get(&pattern) {
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
                        DocKind::Invoice,
                        &pattern,
                        &format!("Imported from {}", source.name()),
                    )?;
                    created.insert(pattern, code.clone());
                    code
                }
            }
        };
        let (_, totals) = skyla_invoicing::compute_totals(
            &self.pack,
            DocKind::Invoice,
            &plan.date,
            self.currency,
            &plan.input.lines,
        )?;
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
        let entry = NewEntry {
            date: plan.date.clone(),
            source_kind: SourceKind::Invoice,
            source_ref: Some(d.number.clone()),
            memo: format!("Faktura {} {}", d.number, d.customer.name),
            created_by: "user".into(),
            lines,
        };
        let uid = uuid::Uuid::new_v5(
            &IMPORT_NAMESPACE,
            format!("{}/{}", source.name(), d.number).as_bytes(),
        );
        let entry_id = create_draft_as(db, &entry, &uid.to_string())?;
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

#[cfg(test)]
mod tests {
    use super::pattern_of;

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
