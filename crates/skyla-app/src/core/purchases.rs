//! Received invoices (improvement wave 3). A purchase is a journal entry
//! like the demo's: source `invoice`, the supplier's number as its
//! reference, debiting expense accounts (with an input-VAT code where VAT
//! is deducted) and 343, crediting 321. The supplier's details sit beside
//! it in `received_invoice`, so the kontrolní hlášení can itemise it with
//! the supplier's DIČ and the bank can settle it like any open payable.
//! VAT is computed with the pack on the tax point, never typed.

use rusqlite::{Connection, OptionalExtension, params};
use skyla_invoicing::{DocKind, LineInput};
use skyla_ledger::{NewEntry, NewLine, PeriodState, SourceKind, create_draft_as, post_entry_at};
use skyla_money::Money;

use super::{Core, money};
use crate::dto::{
    AccountChoiceDto, PurchaseDraftDto, PurchaseDto, PurchaseFormDto, VatCodeChoiceDto,
};
use crate::error::CoreError;

const RECEIVABLE_VAT: &str = "343";
const PAYABLES: &str = "321";

/// The namespace of purchase entries' ids (UUID v5 of `supplier/number`).
const PURCHASE_NAMESPACE: uuid::Uuid = uuid::uuid!("4c2e8f61-9a3d-5b07-8e14-6f0a2d9c7b35");

pub(crate) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS received_invoice (
    entry_id INTEGER PRIMARY KEY,
    supplier TEXT NOT NULL,
    ico TEXT,
    dic TEXT,
    number TEXT NOT NULL,
    issue_date TEXT NOT NULL,
    due_date TEXT
) STRICT;";

fn bad(msg: impl Into<String>) -> CoreError {
    CoreError::BadRequest(msg.into())
}

fn sql(e: rusqlite::Error) -> CoreError {
    CoreError::Ledger(skyla_ledger::LedgerError::from(e))
}

/// Who sent a received invoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Supplier {
    pub(crate) name: String,
    pub(crate) dic: Option<String>,
    pub(crate) due_date: Option<String>,
}

/// A recorded supplier for entry `entry_id`, if any.
pub(crate) fn supplier_of(conn: &Connection, entry_id: i64) -> Result<Option<Supplier>, CoreError> {
    conn.query_row(
        "SELECT supplier, dic, due_date FROM received_invoice WHERE entry_id = ?1",
        [entry_id],
        |r| {
            Ok(Supplier {
                name: r.get(0)?,
                dic: r.get(1)?,
                due_date: r.get(2)?,
            })
        },
    )
    .optional()
    .map_err(sql)
}

/// Every supplier recorded, for pseudonyms.
pub(crate) fn suppliers(conn: &Connection) -> Result<Vec<String>, CoreError> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT supplier FROM received_invoice ORDER BY supplier")
        .map_err(sql)?;
    let rows = stmt
        .query_map([], |r| r.get(0))
        .map_err(sql)?
        .collect::<Result<_, _>>()
        .map_err(sql)?;
    Ok(rows)
}

fn trimmed(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

impl Core {
    fn expense_accounts(&self) -> Vec<AccountChoiceDto> {
        let mut out: Vec<AccountChoiceDto> = self
            .accounts
            .iter()
            .filter(|(code, _)| code.len() == 3 && code.starts_with('5'))
            .map(|(code, (name, _))| AccountChoiceDto {
                code: code.clone(),
                name: name.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.code.cmp(&b.code));
        out
    }

    fn vat_payer(&self) -> bool {
        self.domain.supplier.vat_payer
    }

    /// What the purchase editor offers.
    pub fn purchase_form(&self) -> Result<PurchaseFormDto, CoreError> {
        let today = self.domain.entity.as_of.clone();
        let mut vat_codes = Vec::new();
        if self.vat_payer() {
            for c in self
                .pack
                .vat_codes
                .iter()
                .filter(|c| c.code.starts_with("IN"))
            {
                vat_codes.push(VatCodeChoiceDto {
                    code: c.code.clone(),
                    name: c.name.clone(),
                    rate_percent: self.pack.vat_rate(&c.code, &today)?.normalize().to_string(),
                });
            }
        }
        Ok(PurchaseFormDto {
            accounts: self.expense_accounts(),
            vat_codes,
            vat_payer: self.vat_payer(),
            today,
        })
    }

    /// Records a received invoice: checks it, posts it (approved by the
    /// user), and keeps the supplier's details. Every problem at once.
    pub fn record_purchase(&self, draft: &PurchaseDraftDto) -> Result<PurchaseDto, CoreError> {
        let form = self.purchase_form()?;
        let mut problems = Vec::new();
        let supplier = draft.supplier.trim().to_owned();
        let number = draft.number.trim().to_owned();
        let ico = trimmed(&draft.ico).map(|i| i.replace(' ', ""));
        let dic = trimmed(&draft.dic).map(|d| d.replace(' ', "").to_uppercase());
        if supplier.is_empty() {
            problems.push("enter the supplier's name".to_owned());
        }
        if number.is_empty() {
            problems.push("enter the invoice's number, as the supplier printed it".to_owned());
        }
        if let Some(i) = &ico
            && !skyla_invoicing::valid_ico(i)
        {
            problems.push(format!("the supplier's IČO {i} isn't valid"));
        }
        if let Some(d) = &dic {
            let (country, rest) = d.split_at(d.len().min(2));
            if country.len() != 2
                || !country.bytes().all(|b| b.is_ascii_uppercase())
                || !(2..=12).contains(&rest.len())
                || !rest.bytes().all(|b| b.is_ascii_alphanumeric())
            {
                problems.push(format!(
                    "the supplier's DIČ {d} isn't a VAT number like CZ12345678"
                ));
            }
        }
        let issue = draft.issue_date.trim().to_owned();
        let tax_point = trimmed(&draft.tax_point_date).unwrap_or_else(|| issue.clone());
        let due = trimmed(&draft.due_date);
        for (label, d) in [
            ("issue date", Some(&issue)),
            ("tax point", Some(&tax_point)),
            ("due date", due.as_ref()),
        ] {
            if let Some(d) = d
                && !skyla_ledger::is_iso_date(d)
            {
                problems.push(format!("the {label} {d:?} isn't a date"));
            }
        }
        if skyla_ledger::is_iso_date(&tax_point) {
            let periods = skyla_ledger::list_periods(&self.db())?;
            match periods.iter().find(|p| {
                p.starts_on.as_str() <= tax_point.as_str()
                    && p.ends_on.as_str() >= tax_point.as_str()
            }) {
                None => problems.push(format!(
                    "{tax_point} is outside the periods these books keep"
                )),
                Some(p) if p.state == PeriodState::Closed => {
                    problems.push(format!("{tax_point} is in a closed period"));
                }
                Some(_) => {}
            }
        }
        if draft.lines.is_empty() {
            problems.push("add at least one line".to_owned());
        }
        let mut inputs = Vec::new();
        let mut deducts = false;
        for (i, l) in draft.lines.iter().enumerate() {
            let n = i + 1;
            if !form.accounts.iter().any(|a| a.code == l.account) {
                problems.push(format!(
                    "line {n}: account {} isn't an expense account",
                    l.account
                ));
            }
            let code = l.vat_code.clone().filter(|c| !c.is_empty());
            match &code {
                Some(c) if !form.vat_codes.iter().any(|v| &v.code == c) => {
                    problems.push(format!("line {n}: VAT code {c:?} isn't offered"));
                }
                Some(_) => deducts = true,
                None => {}
            }
            let base = skyla_money::parse_amount_cs(&l.base, self.currency);
            match &base {
                Ok(b) if b.minor() > 0 => {}
                Ok(_) => problems.push(format!("line {n}: the amount must be above zero")),
                Err(_) => problems.push(format!(
                    "line {n}: amount {:?} isn't an amount like 1 200,00",
                    l.base
                )),
            }
            inputs.push((
                l.account.clone(),
                code,
                base.map(|b| b.minor()).unwrap_or_default(),
                l.description.trim().to_owned(),
            ));
        }
        if deducts && dic.is_none() {
            problems.push(
                "deducting VAT needs the supplier's DIČ, as printed on the invoice".to_owned(),
            );
        }
        if !supplier.is_empty() && !number.is_empty() {
            let exists: bool = self
                .db()
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM received_invoice WHERE lower(supplier) = lower(?1) AND number = ?2)",
                    params![supplier, number],
                    |r| r.get(0),
                )
                .map_err(sql)?;
            if exists {
                problems.push(format!("{number} from {supplier} is already recorded"));
            }
        }
        // VAT per code, with the pack's rate and rounding on the tax point.
        let vat_lines: Vec<LineInput> = inputs
            .iter()
            .filter_map(|(_, code, base, _)| {
                code.as_ref().map(|c| LineInput {
                    description: String::new(),
                    quantity: "1".into(),
                    unit: String::new(),
                    unit_price_minor: *base,
                    vat_code: c.clone(),
                    account: None,
                })
            })
            .collect();
        let recap = if problems.is_empty() && !vat_lines.is_empty() {
            skyla_invoicing::compute_totals(
                &self.pack,
                DocKind::Invoice,
                &tax_point,
                self.currency,
                &vat_lines,
            )?
            .1
            .recap
        } else {
            Vec::new()
        };
        let vat_total: i64 = recap.iter().map(|r| r.vat.minor()).sum();
        if let Some(stated) = trimmed(&draft.stated_vat) {
            match skyla_money::parse_amount_cs(&stated, self.currency) {
                Ok(s) if problems.is_empty() && s.minor() != vat_total => problems.push(format!(
                    "the invoice states VAT {stated}, but its bases at the pack's rates give {}: check the rates and amounts",
                    Money::new(vat_total, self.currency).format_cs()
                )),
                Ok(_) => {}
                Err(_) => problems.push(format!("the stated VAT {stated:?} isn't an amount like 210,00")),
            }
        }
        if !problems.is_empty() {
            return Err(CoreError::Invoicing(
                skyla_invoicing::InvoicingError::Invalid(problems),
            ));
        }

        let currency = self.currency;
        let mut lines = Vec::new();
        let mut gross = 0_i64;
        for (account, code, base, description) in &inputs {
            let mut line = NewLine::debit(account, Money::new(*base, currency));
            line.vat_code.clone_from(code);
            line.memo.clone_from(description);
            lines.push(line);
            gross += base;
        }
        for r in &recap {
            if !r.vat.is_zero() {
                let mut line = NewLine::debit(RECEIVABLE_VAT, r.vat);
                line.vat_code = Some(r.vat_code.clone());
                lines.push(line);
                gross += r.vat.minor();
            }
        }
        lines.push(NewLine::credit(PAYABLES, Money::new(gross, currency))?);
        let entry = NewEntry {
            date: tax_point.clone(),
            source_kind: SourceKind::Invoice,
            source_ref: Some(number.clone()),
            memo: format!("Přijatá faktura {number} {supplier}"),
            created_by: "user".into(),
            lines,
        };
        let uid = uuid::Uuid::new_v5(
            &PURCHASE_NAMESPACE,
            format!("{}/{number}", supplier.to_lowercase()).as_bytes(),
        );
        let posted_at = format!("{}T12:00:00.000Z", self.domain.entity.as_of);
        let entry_id = {
            let db = self.db();
            db.execute_batch("SAVEPOINT purchase").map_err(sql)?;
            let result = (|| -> Result<i64, CoreError> {
                let id = create_draft_as(&db, &entry, &uid.to_string())?;
                post_entry_at(&db, id, Some("user"), Some(&posted_at))?;
                db.execute(
                    "INSERT INTO received_invoice (entry_id, supplier, ico, dic, number, issue_date, due_date)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![id, supplier, ico, dic, number, issue, due],
                )
                .map_err(sql)?;
                Ok(id)
            })();
            match result {
                Ok(id) => {
                    db.execute_batch("RELEASE purchase").map_err(sql)?;
                    id
                }
                Err(e) => {
                    let _ = db.execute_batch("ROLLBACK TO purchase; RELEASE purchase");
                    return Err(e);
                }
            }
        };
        self.purchases()?
            .into_iter()
            .find(|p| p.entry_id == entry_id)
            .ok_or_else(|| bad("the purchase was posted but can't be read back"))
    }

    /// Every received invoice: entries crediting payables, with what's paid.
    pub fn purchases(&self) -> Result<Vec<PurchaseDto>, CoreError> {
        let as_of = skyla_rules::date::parse(&self.domain.entity.as_of)
            .ok_or_else(|| bad("the books' date isn't a date"))?;
        let db = self.db();
        let mut out = Vec::new();
        for e in skyla_ledger::list_posted(&db, "1900-01-01", "2999-12-31")? {
            if e.source_kind != SourceKind::Invoice {
                continue;
            }
            let owed: i64 = e
                .lines
                .iter()
                .filter(|l| l.account == PAYABLES)
                .map(|l| -l.functional.minor())
                .sum();
            if owed <= 0 {
                continue;
            }
            let number = e.source_ref.clone().unwrap_or_else(|| format!("#{}", e.id));
            let recorded = supplier_of(&db, e.id)?;
            let demo = self.domain.purchases.iter().find(|p| p.reference == number);
            let supplier = recorded
                .as_ref()
                .map(|s| s.name.clone())
                .or_else(|| demo.map(|p| p.supplier.clone()))
                .unwrap_or_else(|| {
                    e.memo
                        .split_once(number.as_str())
                        .map_or(e.memo.as_str(), |(_, rest)| rest)
                        .trim()
                        .to_owned()
                });
            let dic = recorded
                .as_ref()
                .and_then(|s| s.dic.clone())
                .or_else(|| demo.and_then(|p| p.vat_id.clone()));
            let due_on = recorded.as_ref().and_then(|s| s.due_date.clone());
            let vat: i64 = e
                .lines
                .iter()
                .filter(|l| l.account == RECEIVABLE_VAT)
                .map(|l| l.functional.minor())
                .sum();
            let paid: i64 = skyla_ledger::settlements_of(&db, e.id)?
                .iter()
                .map(|s| s.amount.minor())
                .sum();
            let open = owed - paid;
            let overdue = due_on
                .as_deref()
                .and_then(skyla_rules::date::parse)
                .map(|d| as_of - d)
                .filter(|days| *days > 0 && open > 0);
            out.push(PurchaseDto {
                entry_id: e.id,
                number,
                supplier,
                dic,
                issued_on: e.date.clone(),
                due_on,
                base: money(Money::new(owed - vat, self.currency))?,
                vat: money(Money::new(vat, self.currency))?,
                gross: money(Money::new(owed, self.currency))?,
                paid: money(Money::new(paid, self.currency))?,
                open: money(Money::new(open, self.currency))?,
                status: if open <= 0 {
                    "paid"
                } else if overdue.is_some() {
                    "overdue"
                } else {
                    "open"
                }
                .into(),
                days_overdue: overdue,
            });
        }
        out.sort_by(|a, b| {
            b.issued_on
                .cmp(&a.issued_on)
                .then(b.entry_id.cmp(&a.entry_id))
        });
        Ok(out)
    }
}
