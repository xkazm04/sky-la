use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;
use skyla_ledger::{
    AccountKind, balance_sheet, cash_basis, list_periods, list_posted, profit_and_loss,
    trial_balance, verify_chain,
};
use skyla_money::{Currency, Money, vat};
use skyla_rules::{Pack, RowPart};

use crate::CoreError;
use crate::demo::{Domain, DomainEntry, demo_domain, demo_ledger};
use crate::dto::*;

/// Accounts that hold VAT in the chart. A chart property, not a statutory
/// value: CZ books VAT on 343 (and its analytic sub-accounts).
const VAT_ACCOUNTS: &[&str] = &["343"];

/// The namespace of the demo's draft and posting ids (UUID v5).
const DEMO_POSTING_NAMESPACE: uuid::Uuid = uuid::uuid!("5d2b8f61-0c7e-5a93-b4d1-7e3f9a2c6b08");

mod advisor;
mod bank;
pub mod egress;
mod entity;
mod explain;
mod export;
pub mod findings;
mod imports;
mod inbox;
mod periods;
mod persist;
mod purchases;
mod recurring;
pub mod refdata;
mod tax;
pub mod toolhost;
pub mod tools;
pub mod update;

/// The application core: one open entity and its ledger.
pub struct Core {
    conn: Mutex<Connection>,
    domain: Domain,
    accounts: HashMap<String, (String, AccountKind)>,
    currency: Currency,
    pack: Pack,
    /// Scheduled drafts' issue dates.
    scheduled: HashMap<i64, String>,
    /// Reference data (ČNB rates, repo history) and pack updates.
    refdata: Mutex<refdata::RefData>,
    /// The opt-in update check.
    updates: Mutex<update::UpdateState>,
    /// Drafts created in this session (the demo derives their ids from it).
    drafts_created: std::sync::atomic::AtomicU64,
    /// The bank workbench: imports, rules, bookings.
    bank: Mutex<bank::BankState>,
    /// The model provider advisors run on.
    provider: Mutex<Box<dyn skyla_advisor::LlmProvider>>,
    /// Proposals advisors filed through their tools (posted only when the
    /// user approves one).
    advisor_inbox: Mutex<inbox::AdvisorInbox>,
    /// What the user lets each advisor task do.
    egress_policies: Mutex<egress::Policies>,
    /// The `skyla-mcp` shim advisors' runs start.
    shim: Mutex<std::path::PathBuf>,
    /// For a real entity: where its books and backups are, and the key.
    real: Option<entity::RealEntity>,
}

/// A new customer as typed, trimmed and checked; problems go to `problems`.
fn new_customer(new: &ClientDto, known: &[ClientDto], problems: &mut Vec<String>) -> ClientDto {
    let trimmed = |s: &Option<String>| {
        s.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let fresh = ClientDto {
        name: new.name.trim().to_owned(),
        ico: trimmed(&new.ico).map(|i| i.replace(' ', "")),
        dic: trimmed(&new.dic).map(|d| d.replace(' ', "").to_uppercase()),
        address: trimmed(&new.address),
    };
    if fresh.name.is_empty() {
        problems.push("enter the new customer's name".into());
    } else if known
        .iter()
        .any(|c| c.name.to_lowercase() == fresh.name.to_lowercase())
    {
        problems.push(format!(
            "{} is already a customer: pick them from the list",
            fresh.name
        ));
    }
    if fresh.address.is_none() {
        problems.push("enter the new customer's address, as printed on the invoice".into());
    }
    if let Some(ico) = &fresh.ico
        && !skyla_invoicing::valid_ico(ico)
    {
        problems.push(format!("the new customer's IČO {ico} isn't valid"));
    }
    if let Some(dic) = &fresh.dic {
        let (country, rest) = dic.split_at(dic.len().min(2));
        if country.len() != 2
            || !country.bytes().all(|b| b.is_ascii_uppercase())
            || !(2..=12).contains(&rest.len())
            || !rest.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            problems.push(format!(
                "the new customer's DIČ {dic} isn't a VAT number like CZ12345678"
            ));
        }
    }
    fresh
}

fn money(m: Money) -> Result<MoneyDto, CoreError> {
    MoneyDto::try_from(m)
}

fn interest_dto(i: skyla_invoicing::LateInterest) -> Result<LateInterestDto, CoreError> {
    Ok(LateInterestDto {
        delay_from: i.delay_from,
        rate_date: i.rate_date,
        annual_rate: i.annual_rate.normalize().to_string(),
        periods: i
            .periods
            .into_iter()
            .map(|p| {
                Ok(InterestPeriodDto {
                    from: p.from,
                    to: p.to,
                    days: p.days,
                    principal: money(p.principal)?,
                    interest: money(p.interest)?,
                })
            })
            .collect::<Result<_, CoreError>>()?,
        total: money(i.total)?,
        recovery_cost: money(i.recovery_cost)?,
    })
}

fn snapshot(s: skyla_ledger::Snapshot) -> SnapshotDto {
    SnapshotDto {
        entries: u32::try_from(s.entries).unwrap_or(u32::MAX),
        last_posted_seq: s.last_posted_seq,
        hash: s.hash,
    }
}

fn lines(lines: Vec<skyla_ledger::StatementLine>) -> Result<Vec<StatementLineDto>, CoreError> {
    lines
        .into_iter()
        .map(|l| {
            Ok(StatementLineDto {
                code: l.code,
                name_cs: l.name_cs,
                name_en: l.name_en,
                amount: money(l.amount)?,
            })
        })
        .collect()
}

/// Days from 1970-01-01 for an ISO date (proleptic Gregorian).
fn day_number(date: &str) -> Result<i64, CoreError> {
    let bad = || CoreError::BadRequest(format!("invalid date {date:?}"));
    if !skyla_ledger::is_iso_date(date) {
        return Err(bad());
    }
    let part = |range: std::ops::Range<usize>| -> Result<i64, CoreError> {
        date.get(range).and_then(|p| p.parse().ok()).ok_or_else(bad)
    };
    let (y, m, d) = (part(0..4)?, part(5..7)?, part(8..10)?);
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Ok(era * 146_097 + doe - 719_468)
}

fn kind_name(kind: AccountKind) -> String {
    kind.as_str().to_owned()
}

impl Core {
    /// Opens the demo entity (WP-09). A real entity opens with
    /// [`Core::open_entity`] after the vault unlocks (WP-30).
    pub fn demo() -> Result<Self, CoreError> {
        let conn = demo_ledger()?;
        let domain = demo_domain()?;
        let pack = Pack::cz_2026()?;
        let scheduled = crate::demo::seed_invoicing(&conn, &pack, &domain)?;
        let core = Self::assemble_demo(conn, domain, scheduled)?;
        // The demo's first October statement, imported but not yet booked.
        core.import_bytes(
            "csob-2026-10-06.xml",
            crate::demo::FIRST_STATEMENT.as_bytes(),
        )?;
        // The demo's past advisor runs, through the real gate.
        core.seed_demo_register()?;
        Ok(core)
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        // A poisoned lock means a panic mid-command; the connection itself is
        // still consistent (every write is a savepoint), so keep serving.
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn account_name(&self, code: &str) -> String {
        self.accounts
            .get(code)
            .map(|(name, _)| name.clone())
            .unwrap_or_default()
    }

    fn amount(&self, minor: i64) -> Result<MoneyDto, CoreError> {
        money(Money::new(minor, self.currency))
    }

    /// Build information.
    pub fn app_info(&self) -> AppInfo {
        AppInfo {
            name: "sky-la".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }

    /// The open entity.
    pub fn entity(&self) -> EntityDto {
        let e = &self.domain.entity;
        EntityDto {
            display_name: e.display_name.clone(),
            legal_form: e.legal_form.clone(),
            vat_period: e.vat_period.clone(),
            functional_currency: e.functional_currency.clone(),
            as_of: e.as_of.clone(),
            bank_name: e.bank_name.clone(),
        }
    }

    /// Accounting periods, oldest first.
    pub fn periods(&self) -> Result<Vec<PeriodDto>, CoreError> {
        let periods = list_periods(&self.db())?;
        Ok(periods
            .into_iter()
            .map(|p| {
                let quarter = match (p.starts_on.get(5..), p.ends_on.get(5..)) {
                    (Some("01-01"), Some("03-31")) => Some(1),
                    (Some("04-01"), Some("06-30")) => Some(2),
                    (Some("07-01"), Some("09-30")) => Some(3),
                    (Some("10-01"), Some("12-31")) => Some(4),
                    _ => None,
                };
                let year = p.starts_on.get(..4).unwrap_or_default().to_owned();
                PeriodDto {
                    id: p.id,
                    label: quarter.map_or_else(
                        || format!("{} – {}", p.starts_on, p.ends_on),
                        |q| format!("Q{q} {year}"),
                    ),
                    starts_on: p.starts_on,
                    ends_on: p.ends_on,
                    state: p.state.as_str().to_owned(),
                }
            })
            .collect())
    }

    /// Profit and loss for `from..=to`.
    pub fn profit_and_loss(&self, from: &str, to: &str) -> Result<ProfitAndLossDto, CoreError> {
        let r = profit_and_loss(&self.db(), from, to)?;
        Ok(ProfitAndLossDto {
            from: r.from,
            to: r.to,
            revenue: lines(r.revenue)?,
            expenses: lines(r.expenses)?,
            total_revenue: money(r.total_revenue)?,
            total_expenses: money(r.total_expenses)?,
            profit: money(r.profit)?,
            snapshot: snapshot(r.snapshot),
        })
    }

    /// Balance sheet as of a date.
    pub fn balance_sheet(&self, as_of: &str) -> Result<BalanceSheetDto, CoreError> {
        let r = balance_sheet(&self.db(), as_of)?;
        Ok(BalanceSheetDto {
            balances: r.balances(),
            as_of: r.as_of,
            assets: lines(r.assets)?,
            liabilities: lines(r.liabilities)?,
            equity: lines(r.equity)?,
            unclosed_profit: money(r.unclosed_profit)?,
            total_assets: money(r.total_assets)?,
            total_liabilities: money(r.total_liabilities)?,
            total_equity: money(r.total_equity)?,
            snapshot: snapshot(r.snapshot),
        })
    }

    /// Trial balance.
    pub fn trial_balance(
        &self,
        from: Option<&str>,
        to: &str,
    ) -> Result<TrialBalanceDto, CoreError> {
        let r = trial_balance(&self.db(), from, to)?;
        Ok(TrialBalanceDto {
            from: r.from,
            to: r.to,
            rows: r
                .rows
                .into_iter()
                .map(|row| {
                    Ok(TrialBalanceRowDto {
                        code: row.code,
                        name_cs: row.name_cs,
                        name_en: row.name_en,
                        kind: kind_name(row.kind),
                        debit: money(row.debit)?,
                        credit: money(row.credit)?,
                        balance: money(row.balance)?,
                    })
                })
                .collect::<Result<_, CoreError>>()?,
            total_debit: money(r.total_debit)?,
            total_credit: money(r.total_credit)?,
            snapshot: snapshot(r.snapshot),
        })
    }

    /// Cash basis for payments dated `from..=to`.
    pub fn cash_basis(&self, from: &str, to: &str) -> Result<CashBasisDto, CoreError> {
        let r = cash_basis(&self.db(), from, to)?;
        Ok(CashBasisDto {
            from: r.from,
            to: r.to,
            lines: r
                .lines
                .into_iter()
                .map(|l| {
                    Ok(CashBasisLineDto {
                        date: l.date,
                        cash_entry_id: l.cash_entry_id,
                        settled_entry_id: l.settled_entry_id,
                        account: l.account,
                        direction: l.direction.as_str().to_owned(),
                        tax_treatment: l.tax_treatment.as_str().to_owned(),
                        amount: money(l.amount)?,
                    })
                })
                .collect::<Result<_, CoreError>>()?,
            totals: r
                .totals
                .into_iter()
                .map(|t| {
                    Ok(CashBasisTotalDto {
                        direction: t.direction.as_str().to_owned(),
                        tax_treatment: t.tax_treatment.as_str().to_owned(),
                        amount: money(t.amount)?,
                    })
                })
                .collect::<Result<_, CoreError>>()?,
            taxable_income: money(r.taxable_income)?,
            deductible_expenses: money(r.deductible_expenses)?,
            snapshot: snapshot(r.snapshot),
        })
    }

    /// Posted entries dated `from..=to`, in posting order.
    pub fn journal(&self, from: &str, to: &str) -> Result<Vec<JournalEntryDto>, CoreError> {
        let entries = list_posted(&self.db(), from, to)?;
        entries
            .into_iter()
            .map(|e| {
                Ok(JournalEntryDto {
                    id: e.id,
                    date: e.date,
                    posted_seq: e.posted_seq,
                    source_kind: e.source_kind.as_str().to_owned(),
                    source_ref: e.source_ref,
                    memo: e.memo,
                    approved_by: e.approved_by,
                    reverses_id: e.reverses_id,
                    lines: e
                        .lines
                        .into_iter()
                        .map(|l| {
                            Ok(JournalLineDto {
                                line_no: l.line_no,
                                account_name: self.account_name(&l.account),
                                account: l.account,
                                amount: money(l.amount)?,
                                functional: money(l.functional)?,
                                fx_rate: l.fx_rate,
                                vat_code: l.vat_code,
                                memo: l.memo,
                            })
                        })
                        .collect::<Result<_, CoreError>>()?,
                })
            })
            .collect()
    }

    /// Hash chain and balance, for the status line.
    pub fn integrity(&self) -> Result<IntegrityDto, CoreError> {
        let db = self.db();
        let chain = verify_chain(&db)?;
        let to = self.domain.entity.as_of.clone();
        let balanced = trial_balance(&db, None, &to)?.balances();
        Ok(IntegrityDto {
            chain_intact: chain.is_intact(),
            entries_checked: u32::try_from(chain.entries_checked).unwrap_or(u32::MAX),
            head: chain.head,
            first_break: chain.first_break.map(|b| b.to_string()),
            balanced,
        })
    }

    /// Invoices from the invoicing module, newest first, drafts on top.
    /// Totals were fixed with the rule pack when issued; paid and credited
    /// amounts come from the ledger's settlement links.
    pub fn invoices(&self) -> Result<Vec<InvoiceDto>, CoreError> {
        use skyla_invoicing::Settlement;
        let db = self.db();
        let as_of = day_number(&self.domain.entity.as_of)?;
        let ids: Vec<i64> = db
            .prepare(
                "SELECT id FROM document WHERE kind = 'invoice'
                 ORDER BY status = 'issued', number DESC, id DESC",
            )
            .map_err(skyla_ledger::LedgerError::from)?
            .query_map([], |r| r.get(0))
            .map_err(skyla_ledger::LedgerError::from)?
            .collect::<Result<_, _>>()
            .map_err(skyla_ledger::LedgerError::from)?;
        let mut out = Vec::new();
        for id in ids {
            let doc = skyla_invoicing::get(&db, &self.pack, id)?;
            let st = skyla_invoicing::state(&db, &self.pack, id)?;
            let overdue = match (&doc.due_date, doc.issued) {
                (Some(due), true) if !st.open.is_zero() && !st.open.is_negative() => {
                    Some(as_of - day_number(due)?).filter(|d| *d > 0)
                }
                _ => None,
            };
            let status = match st.settlement {
                Settlement::Draft if self.scheduled.contains_key(&id) => "scheduled",
                Settlement::Draft => "draft",
                Settlement::Paid => "paid",
                Settlement::Credited => "credited",
                _ if overdue.is_some() => "overdue",
                Settlement::PartlySettled => "partPaid",
                Settlement::Open => "open",
            };
            let lines = doc
                .lines
                .iter()
                .map(|l| {
                    let on = doc
                        .tax_point_date
                        .clone()
                        .unwrap_or_else(|| self.domain.entity.as_of.clone());
                    let rate = self.pack.vat_rate(&l.input.vat_code, &on)?;
                    let rounding = self.pack.rounding("vat.rounding.document", &on)?;
                    Ok(InvoiceLineDto {
                        description: l.input.description.clone(),
                        quantity: l.input.quantity.clone(),
                        unit: l.input.unit.clone(),
                        unit_price: self.amount(l.input.unit_price_minor)?,
                        vat_code: l.input.vat_code.clone(),
                        vat_rate_percent: rate.to_string(),
                        base: money(l.amount)?,
                        vat: money(vat::from_base(l.amount, rate, rounding)?.vat)?,
                    })
                })
                .collect::<Result<_, CoreError>>()?;
            out.push(InvoiceDto {
                id,
                number: doc.number.clone(),
                client: doc.customer.name.clone(),
                status: status.into(),
                issued_on: doc.issue_date.clone(),
                due_on: doc.due_date.clone(),
                scheduled_for: self.scheduled.get(&id).cloned(),
                paid_on: if st.settlement == Settlement::Paid {
                    st.settled_on.clone()
                } else {
                    None
                },
                days_overdue: overdue,
                base: money(doc.totals.base)?,
                vat: money(doc.totals.vat)?,
                gross: money(doc.totals.gross)?,
                paid: money(st.paid)?,
                credited: money(st.credited)?,
                open: money(st.open)?,
                lines,
                entry_id: doc.entry_id,
                pack: doc.pack.clone(),
            });
        }
        Ok(out)
    }

    /// Renders a document to PDF in `lang` (`cs` or `en`). The amount due
    /// (and the QR Platba code) is the total less deducted advances, as on
    /// the document when it was issued.
    pub fn invoice_pdf(&self, id: i64, lang: &str) -> Result<DocumentPdfDto, CoreError> {
        use base64::Engine as _;
        let lang = match lang {
            "cs" => skyla_render::Lang::Cs,
            "en" => skyla_render::Lang::En,
            other => return Err(CoreError::BadRequest(format!("unknown language {other:?}"))),
        };
        let db = self.db();
        let doc = skyla_invoicing::get(&db, &self.pack, id)?;
        let st = skyla_invoicing::state(&db, &self.pack, id)?;
        let related = match doc.related_id {
            Some(r) => skyla_invoicing::get(&db, &self.pack, r)?.number,
            None => None,
        };
        drop(db);
        let due = doc.totals.gross.checked_sub(st.advances)?;
        let rendered = skyla_render::invoice_pdf(&doc, due, related.as_deref(), lang)?;
        let title = match (doc.kind, lang) {
            (skyla_invoicing::DocKind::CreditNote, skyla_render::Lang::Cs) => "Opravný doklad",
            (skyla_invoicing::DocKind::CreditNote, skyla_render::Lang::En) => "Credit note",
            (_, skyla_render::Lang::Cs) => "Faktura",
            (_, skyla_render::Lang::En) => "Invoice",
        };
        let number = doc.number.unwrap_or_else(|| format!("draft-{id}"));
        Ok(DocumentPdfDto {
            file_name: format!("{title} {number}.pdf"),
            pdf_base64: base64::engine::general_purpose::STANDARD.encode(&rendered.pdf),
            pages: u32::try_from(rendered.text.len()).unwrap_or(u32::MAX),
            spayd: rendered.spayd,
        })
    }

    /// Writes an issued document in an exchange format: `isdoc` (ISDOC
    /// 6.0.2), `ubl` (UBL 2.1, Peppol BIS Billing 3.0) or `cii` (CII D16B,
    /// EN 16931), with the document it refers to and the advances it deducts.
    pub fn invoice_xml(&self, id: i64, format: &str) -> Result<DocumentXmlDto, CoreError> {
        let db = self.db();
        let doc = skyla_invoicing::get(&db, &self.pack, id)?;
        let related = doc
            .related_id
            .map(|r| skyla_invoicing::get(&db, &self.pack, r))
            .transpose()?;
        let advances = doc
            .advances
            .iter()
            .map(|a| skyla_invoicing::get(&db, &self.pack, *a))
            .collect::<Result<Vec<_>, _>>()?;
        drop(db);
        let input = skyla_invoicing::ExportInput {
            doc: &doc,
            related: related.as_ref(),
            advances: &advances,
        };
        let (xml, suffix) = match format {
            "isdoc" => (skyla_invoicing::to_isdoc(&self.pack, input)?, "isdoc"),
            "ubl" => (skyla_invoicing::to_ubl(&self.pack, input)?, "ubl.xml"),
            "cii" => (skyla_invoicing::to_cii(&self.pack, input)?, "cii.xml"),
            other => return Err(CoreError::BadRequest(format!("unknown format {other:?}"))),
        };
        let number = doc.number.unwrap_or_else(|| format!("draft-{id}"));
        Ok(DocumentXmlDto {
            file_name: format!("{number}.{suffix}"),
            media_type: "application/xml".into(),
            xml,
        })
    }

    /// Reminders due on `as_of` under the user's sequence, drafted in Czech
    /// and English for the user to send.
    pub fn dunning_queue(&self, as_of: &str) -> Result<Vec<DunningNoticeDto>, CoreError> {
        use skyla_invoicing::dunning::Tone;
        let notices = skyla_invoicing::dunning::dunning_queue(
            &self.db(),
            &self.pack,
            &self.repo_rates(),
            as_of,
        )?;
        notices
            .into_iter()
            .map(|n| {
                Ok(DunningNoticeDto {
                    document_id: n.document_id,
                    number: n.number,
                    customer: n.customer,
                    step: n.step,
                    tone: match n.tone {
                        Tone::Friendly => "friendly",
                        Tone::Firm => "firm",
                        Tone::Final => "final",
                    }
                    .into(),
                    due_on: n.due_date,
                    scheduled_on: n.scheduled_on,
                    days_overdue: n.days_overdue,
                    open: money(n.open)?,
                    interest: n.interest.map(interest_dto).transpose()?,
                    interest_problem: n.interest_problem,
                    subject_cs: n.subject_cs,
                    body_cs: n.body_cs,
                    subject_en: n.subject_en,
                    body_en: n.body_en,
                })
            })
            .collect()
    }

    /// The recurring invoice templates and when each runs next.
    pub fn recurring_templates(&self) -> Result<Vec<RecurringTemplateDto>, CoreError> {
        let db = self.db();
        let mut out = Vec::new();
        for t in skyla_invoicing::recurring::templates(&db)? {
            let on = t
                .next
                .clone()
                .unwrap_or_else(|| self.domain.entity.as_of.clone());
            let (_, totals) = skyla_invoicing::compute_totals(
                &self.pack,
                t.input.draft.kind,
                &on,
                self.currency,
                &t.input.draft.lines,
            )?;
            let s = &t.input.schedule;
            out.push(RecurringTemplateDto {
                id: t.id,
                name: t.input.name.clone(),
                client: t.input.draft.customer.name.clone(),
                frequency: serde_json::to_value(s.frequency)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default(),
                interval: s.interval,
                start: s.start.clone(),
                next: t.next.clone(),
                due_days: t.input.due_days,
                auto_issue: t.input.auto_issue,
                active: t.active,
                gross: money(totals.gross)?,
            });
        }
        Ok(out)
    }

    /// What the invoice editor offers: customers, the VAT codes this
    /// supplier may charge, units, terms, and the next number.
    pub fn invoice_form(&self) -> Result<InvoiceFormDto, CoreError> {
        let db = self.db();
        let today = self.domain.entity.as_of.clone();
        let vat_payer = skyla_invoicing::supplier(&db)?.is_some_and(|s| s.vat_payer);
        let clients = skyla_invoicing::customers(&db)?
            .into_iter()
            .map(|c| ClientDto {
                name: c.name,
                ico: c.ico,
                dic: c.dic,
                address: c.address,
            })
            .collect();
        let mut vat_codes = Vec::new();
        for c in &self.pack.vat_codes {
            // Sales codes carry an e-invoice category; the supplier's
            // registration decides which of them apply.
            if c.einvoice.is_some() && c.outside_vat != vat_payer {
                vat_codes.push(VatCodeChoiceDto {
                    code: c.code.clone(),
                    name: c.name.clone(),
                    rate_percent: self.pack.vat_rate(&c.code, &today)?.normalize().to_string(),
                });
            }
        }
        Ok(InvoiceFormDto {
            clients,
            vat_codes,
            units: ["h", "ks", "den", "měs", "km"].map(str::to_owned).to_vec(),
            due_days: vec![7, 14, 21, 30],
            next_number: skyla_invoicing::next_number(&db, "FV", &today)?,
            today,
        })
    }

    /// Saves a draft typed in the editor. Amounts are parsed here, in Czech
    /// formats, and every problem is listed at once.
    pub fn create_invoice_draft(&self, draft: &InvoiceDraftDto) -> Result<InvoiceDto, CoreError> {
        let input = self.draft_input(draft)?;
        // A known id per draft, so the demo's recordings are reproducible.
        let n = self
            .drafts_created
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        let uid = uuid::Uuid::new_v5(&DEMO_POSTING_NAMESPACE, format!("draft/{n}").as_bytes());
        let id = skyla_invoicing::create_draft_as(&self.db(), &input, &uid.to_string())?;
        self.invoice(id)
    }

    /// What the editor typed, checked and parsed into a draft; every
    /// problem listed at once.
    fn draft_input(
        &self,
        draft: &InvoiceDraftDto,
    ) -> Result<skyla_invoicing::DraftInput, CoreError> {
        let form = self.invoice_form()?;
        let mut problems = Vec::new();
        let client = match (
            form.clients.iter().find(|c| c.name == draft.client),
            &draft.new_client,
        ) {
            (Some(known), _) => Some(known.clone()),
            (None, Some(new)) if draft.client.is_empty() => {
                let before = problems.len();
                let fresh = new_customer(new, &form.clients, &mut problems);
                (problems.len() == before).then_some(fresh)
            }
            (None, _) => {
                problems.push(if draft.client.is_empty() {
                    "pick a customer".to_owned()
                } else {
                    format!("{:?} isn't a known customer", draft.client)
                });
                None
            }
        };
        if !form.due_days.contains(&draft.due_days) {
            problems.push(format!(
                "payment terms of {} days aren't offered",
                draft.due_days
            ));
        }
        if draft.lines.is_empty() {
            problems.push("add at least one line".to_owned());
        }
        let mut lines = Vec::new();
        for (i, l) in draft.lines.iter().enumerate() {
            let n = i + 1;
            let quantity = l.quantity.trim().replace(',', ".");
            let quantity_ok = !quantity.is_empty()
                && quantity.chars().all(|c| c.is_ascii_digit() || c == '.')
                && quantity.matches('.').count() <= 1
                && quantity.split('.').nth(1).is_none_or(|f| f.len() <= 4)
                && quantity
                    .parse::<skyla_money::Rate>()
                    .is_ok_and(|q| q > skyla_money::Rate::ZERO);
            if !quantity_ok {
                problems.push(format!(
                    "line {n}: quantity {:?} isn't a positive number",
                    l.quantity
                ));
            }
            let price = skyla_money::parse_amount_cs(&l.unit_price, self.currency);
            match &price {
                Ok(p) if p.minor() > 0 => {}
                Ok(_) => problems.push(format!("line {n}: the unit price must be above zero")),
                Err(_) => problems.push(format!(
                    "line {n}: unit price {:?} isn't an amount like 1 200,00",
                    l.unit_price
                )),
            }
            if !form.vat_codes.iter().any(|c| c.code == l.vat_code) {
                problems.push(format!("line {n}: VAT code {:?} isn't offered", l.vat_code));
            }
            lines.push(skyla_invoicing::LineInput {
                description: l.description.trim().to_owned(),
                quantity,
                unit: l.unit.trim().to_owned(),
                unit_price_minor: price.map(|p| p.minor()).unwrap_or_default(),
                vat_code: l.vat_code.clone(),
                account: None,
            });
        }
        if !problems.is_empty() {
            return Err(CoreError::Invoicing(
                skyla_invoicing::InvoicingError::Invalid(problems),
            ));
        }
        let client = client.unwrap_or_else(|| unreachable!());
        let due = skyla_rules::date::parse(&form.today)
            .map(|d| skyla_rules::date::format(d + i64::from(draft.due_days)));
        let input = skyla_invoicing::DraftInput {
            kind: skyla_invoicing::DocKind::Invoice,
            series: "FV".into(),
            customer: skyla_invoicing::Customer {
                name: client.name,
                ico: client.ico,
                dic: client.dic,
                address: client.address,
            },
            due_date: due,
            tax_point_date: None,
            note: draft.note.trim().to_owned(),
            lines,
            related_id: None,
            advances: Vec::new(),
        };
        Ok(input)
    }

    /// Issues a draft on `issue_date`: assigns the next number and posts it.
    pub fn issue_invoice(&self, id: i64, issue_date: &str) -> Result<InvoiceDto, CoreError> {
        let accounts = skyla_invoicing::Accounts::cz();
        let db = self.db();
        // The demo posts with a known identity and a fixed clock, so its
        // recordings (and the hash chain) come out the same on every run.
        let doc_uid = skyla_invoicing::get(&db, &self.pack, id)?.uid;
        let uid = uuid::Uuid::new_v5(&DEMO_POSTING_NAMESPACE, doc_uid.as_bytes()).to_string();
        let posted_at = format!("{issue_date}T12:00:00.000Z");
        let replay = skyla_invoicing::IssueReplay {
            uid: &uid,
            posted_at: &posted_at,
        };
        skyla_invoicing::issue(&db, &self.pack, &accounts, id, issue_date, Some(replay))?;
        drop(db);
        self.invoice(id)
    }

    /// Deletes a draft. Issued documents never go away.
    pub fn delete_invoice_draft(&self, id: i64) -> Result<(), CoreError> {
        skyla_invoicing::delete_draft(&self.db(), id)?;
        Ok(())
    }

    fn invoice(&self, id: i64) -> Result<InvoiceDto, CoreError> {
        self.invoices()?
            .into_iter()
            .find(|i| i.id == id)
            .ok_or_else(|| CoreError::BadRequest(format!("no invoice {id}")))
    }

    fn proposed_entry(&self, entry: &DomainEntry) -> Result<ProposedEntryDto, CoreError> {
        let (mut debit, mut credit) = (0_i64, 0_i64);
        let mut out = Vec::new();
        for l in &entry.lines {
            if !self.accounts.contains_key(&l.account) {
                return Err(CoreError::Demo(format!(
                    "proposal uses unknown account {}",
                    l.account
                )));
            }
            if l.amount_minor >= 0 {
                debit += l.amount_minor;
            } else {
                credit -= l.amount_minor;
            }
            out.push(ProposedLineDto {
                account: l.account.clone(),
                account_name: self.account_name(&l.account),
                debit: (l.amount_minor >= 0)
                    .then(|| self.amount(l.amount_minor))
                    .transpose()?,
                credit: (l.amount_minor < 0)
                    .then(|| self.amount(-l.amount_minor))
                    .transpose()?,
                vat_code: l.vat_code.clone(),
            });
        }
        // VAT lines must be what the engine computes with the pack, not what
        // the fixture says.
        let rounding = self.pack.rounding("vat.rounding.document", &entry.date)?;
        let rate = |code: &str| {
            self.pack
                .vat_rate(code, &entry.date)
                .map_err(CoreError::from)
        };
        let tax_lines = |positive: bool| -> i64 {
            entry
                .lines
                .iter()
                .filter(|l| {
                    l.vat_code.is_some() && l.account == "343" && (l.amount_minor > 0) == positive
                })
                .map(|l| l.amount_minor)
                .sum()
        };
        if let Some(check) = &entry.reverse_charge {
            let base = Money::new(check.base_minor.unwrap_or_default(), self.currency);
            let split = vat::from_base(base, rate(&check.vat_code)?, rounding)?;
            if tax_lines(true) != split.vat.minor() || -tax_lines(false) != split.vat.minor() {
                return Err(CoreError::Demo(format!(
                    "reverse charge on {}: VAT should be {:?}",
                    entry.memo, split.vat
                )));
            }
        }
        if let Some(check) = &entry.vat_split {
            let gross = Money::new(check.gross_minor.unwrap_or_default(), self.currency);
            let split = vat::from_gross(gross, rate(&check.vat_code)?, rounding)?;
            if tax_lines(true) != split.vat.minor() {
                return Err(CoreError::Demo(format!(
                    "VAT split on {}: VAT should be {:?}",
                    entry.memo, split.vat
                )));
            }
        }
        Ok(ProposedEntryDto {
            date: entry.date.clone(),
            memo: entry.memo.clone(),
            lines: out,
            total_debit: self.amount(debit)?,
            total_credit: self.amount(credit)?,
            balanced: debit == credit,
        })
    }

    /// The demo proposal about a bank line, if there is one.
    fn proposal_for_line(&self, line_id: &str) -> Option<String> {
        self.domain
            .proposals
            .iter()
            .find(|p| p.bank_line_id.as_deref() == Some(line_id))
            .map(|p| p.id.clone())
    }

    /// The inbox: proposals waiting for a human decision, and bank lines
    /// the workbench couldn't settle on its own.
    pub fn proposals(&self) -> Result<Vec<ProposalDto>, CoreError> {
        let lines = self.bank_statement()?.lines;
        let open_invoices: HashMap<String, i64> = self
            .invoices()?
            .into_iter()
            .filter_map(|i| i.number.map(|n| (n, i.open.minor)))
            .collect();
        let mut items: Vec<ProposalDto> = self
            .domain
            .proposals
            .iter()
            .map(|p| {
                let entry = p
                    .entry
                    .as_ref()
                    .map(|e| self.proposed_entry(e))
                    .transpose()?;
                for settle in p.entry.iter().flat_map(|e| &e.settles) {
                    let open = open_invoices
                        .get(&settle.invoice)
                        .copied()
                        .unwrap_or_default();
                    if settle.amount_minor > open {
                        return Err(CoreError::Demo(format!(
                            "proposal {} settles {} of invoice {}, which has {} open",
                            p.id, settle.amount_minor, settle.invoice, open
                        )));
                    }
                }
                let amount = p
                    .bank_line_id
                    .as_ref()
                    .and_then(|id| lines.iter().find(|l| &l.id == id))
                    .map(|l| l.amount.clone());
                Ok(ProposalDto {
                    id: p.id.clone(),
                    kind: p.kind.clone(),
                    title: p.title.clone(),
                    detail: p.detail.clone(),
                    confidence: p.confidence.clone(),
                    source_kind: p.source_kind.clone(),
                    source: p.source.clone(),
                    bank_line_id: p.bank_line_id.clone(),
                    due_on: match &p.deadline {
                        Some(d) => Some(self.pack.deadline_after(&d.key, &d.period_end)?),
                        None => p.due_on.clone(),
                    },
                    amount,
                    entry,
                    reasons: p.reasons.clone(),
                })
            })
            .collect::<Result<Vec<_>, CoreError>>()?;
        // Lines the workbench couldn't settle on its own, without a proposal.
        for l in lines
            .iter()
            .filter(|l| l.status == "needs_you" && l.proposal_id.is_none())
        {
            items.push(ProposalDto {
                id: format!("bank-{}", l.id),
                kind: "posting".into(),
                title: format!("{} · bank line needs you", l.counterparty),
                detail: l
                    .held_because
                    .clone()
                    .unwrap_or_else(|| "Nothing in the books fits this line yet.".into()),
                confidence: Some("needs_you".into()),
                source_kind: "rule".into(),
                source: "Bank workbench".into(),
                bank_line_id: Some(l.id.clone()),
                due_on: None,
                amount: Some(l.amount.clone()),
                entry: None,
                reasons: l.held_because.iter().cloned().collect(),
            });
        }
        // The financial advisor's actionable findings (the rest are on the
        // Advisors screen): a big swing, a duplicated charge, a short runway.
        for f in self.financial_findings()? {
            if !(f.detector == "variance"
                || f.detector == "runway"
                || f.id.starts_with("duplicate:"))
            {
                continue;
            }
            let cited: Vec<String> = f.cites.iter().map(|c| format!("#{c}")).collect();
            items.push(ProposalDto {
                id: format!("finding-{}", f.id),
                kind: "advice".into(),
                title: f.title.clone(),
                detail: format!("Financial advisor · {}", f.detail),
                confidence: None,
                source_kind: "advisor".into(),
                source: "Financial advisor".into(),
                bank_line_id: None,
                due_on: None,
                amount: None,
                entry: None,
                reasons: vec![
                    "A finding from the books for your review; nothing to post.".into(),
                    format!("Entries: {}", cited.join(", ")),
                ],
            });
        }
        items.extend(self.upcoming_deadlines()?);
        // What advisors filed through their tools, waiting for review.
        let inbox = self.inbox();
        items.extend(inbox.proposals().cloned());
        // A proposal about a line that's booked now is done, and dismissed
        // advice stays dismissed.
        let booked = |id: &Option<String>| {
            id.as_ref()
                .and_then(|id| lines.iter().find(|l| &l.id == id))
                .is_some_and(|l| l.status == "booked")
        };
        items.retain(|p| !booked(&p.bank_line_id) && !inbox.is_dismissed(&p.id));
        Ok(items)
    }

    /// The calendar's deadlines in the next month, one item per period and
    /// due date (a return and its control statement due together are one).
    fn upcoming_deadlines(&self) -> Result<Vec<ProposalDto>, CoreError> {
        let as_of = self.domain.entity.as_of.clone();
        let Some(today) = skyla_rules::date::parse(&as_of) else {
            return Ok(Vec::new());
        };
        let horizon = skyla_rules::date::format(today + 31);
        let Some(year) = as_of.get(..4).and_then(|y| y.parse::<i32>().ok()) else {
            return Ok(Vec::new());
        };
        let mut due: Vec<ObligationDto> = Vec::new();
        for y in [year - 1, year, year + 1] {
            for o in self.obligations(y)? {
                if o.due >= as_of
                    && o.due <= horizon
                    && !due
                        .iter()
                        .any(|d| d.obligation == o.obligation && d.period == o.period)
                {
                    due.push(o);
                }
            }
        }
        due.sort_by(|a, b| a.due.cmp(&b.due).then(a.name.cmp(&b.name)));
        let mut items: Vec<ProposalDto> = Vec::new();
        let mut groups: Vec<Vec<ObligationDto>> = Vec::new();
        for o in due {
            match groups
                .iter_mut()
                .find(|g| g[0].due == o.due && g[0].period == o.period)
            {
                Some(g) => g.push(o),
                None => groups.push(vec![o]),
            }
        }
        for g in groups {
            let first = &g[0];
            let names: Vec<&str> = g.iter().map(|o| o.name.as_str()).collect();
            let mut reasons: Vec<String> = g
                .iter()
                .map(|o| format!("{}: {}", o.name, o.citation))
                .collect();
            if first.shifted {
                reasons.push(format!(
                    "The nominal deadline, {}, isn't a working day, so it moves to {} (daňový řád § 33 odst. 4).",
                    first.nominal, first.due
                ));
            }
            items.push(ProposalDto {
                id: format!("deadline-{}-{}", first.period, first.due),
                kind: "deadline".into(),
                title: format!("{} · {}", names.join(", "), periods::label(&first.period)),
                detail: if g.iter().any(|o| o.action != "pay") {
                    "From the obligations calendar; prepare it in Taxes"
                } else {
                    "From the obligations calendar; a payment to make"
                }
                .into(),
                confidence: None,
                source_kind: "calendar".into(),
                source: format!("Rule pack {}", self.pack.provenance()),
                bank_line_id: None,
                due_on: Some(first.due.clone()),
                amount: None,
                entry: None,
                reasons,
            });
        }
        Ok(items)
    }

    /// The DPH return for `from..=to`: the ledger's VAT postings mapped onto
    /// form rows by the rule pack, with the payable amount and the deadline.
    pub fn vat_return(&self, from: &str, to: &str) -> Result<VatReturnDto, CoreError> {
        let rules: Vec<skyla_ledger::VatRowRule> = self
            .pack
            .vat_codes
            .iter()
            .flat_map(|code| {
                code.rows.iter().map(|m| skyla_ledger::VatRowRule {
                    vat_code: code.code.clone(),
                    row: m.row.clone(),
                    part: match m.part {
                        RowPart::Base => skyla_ledger::VatPart::Base,
                        RowPart::Tax => skyla_ledger::VatPart::Tax,
                        RowPart::TaxDebit => skyla_ledger::VatPart::TaxDebit,
                        RowPart::TaxCredit => skyla_ledger::VatPart::TaxCredit,
                    },
                    credit_positive: m.credit_positive,
                })
            })
            .collect();
        let ledger = skyla_ledger::vat_ledger(&self.db(), from, to, VAT_ACCOUNTS, &rules)?;
        // A row is on the output side when its tax is shown credit-positive.
        let output_side = |row: &str| {
            self.pack
                .vat_codes
                .iter()
                .flat_map(|c| &c.rows)
                .any(|m| m.row == row && m.part != RowPart::Base && m.credit_positive)
        };
        let label = |row: &str| {
            self.pack
                .vat_codes
                .iter()
                .filter(|c| c.rows.iter().any(|m| m.row == row))
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        };
        let (mut output, mut input) = (Money::zero(self.currency), Money::zero(self.currency));
        let mut rows = Vec::new();
        for r in ledger.rows {
            let out = output_side(&r.row);
            if out {
                output = output.checked_add(r.tax)?;
            } else {
                input = input.checked_add(r.tax)?;
            }
            if r.base.is_zero() && r.tax.is_zero() {
                continue;
            }
            rows.push(VatReturnRowDto {
                label: label(&r.row),
                side: if out { "output" } else { "input" }.into(),
                base: money(r.base)?,
                tax: money(r.tax)?,
                row: r.row,
            });
        }
        rows.sort_by_key(|r| r.row.parse::<u32>().unwrap_or(u32::MAX));
        Ok(VatReturnDto {
            from: ledger.from,
            to: ledger.to,
            pack: self.pack.provenance(),
            pack_review: format!("{:?}", self.pack.info.review).to_lowercase(),
            rows,
            output_tax: money(output)?,
            input_tax: money(input)?,
            payable: money(output.checked_sub(input)?)?,
            unmapped: ledger.unmapped,
            due_on: self
                .pack
                .deadline_after("vat.return.due_days_after_period", to)?,
            snapshot: snapshot(ledger.snapshot),
        })
    }

    /// The rule pack in force, with every value effective on the as-of date.
    pub fn rule_pack(&self) -> RulePackDto {
        let on = &self.domain.entity.as_of;
        let values = self
            .pack
            .keys()
            .into_iter()
            .filter_map(|key| self.pack.value(key, on).ok())
            .map(|v| PackValueDto {
                key: v.key.clone(),
                kind: format!("{:?}", v.kind).to_lowercase(),
                value: v.value.clone(),
                effective_from: v.effective_from.clone(),
                effective_to: v.effective_to.clone(),
                citation: self.pack.citation(&v.cite),
                url: self
                    .pack
                    .acts
                    .get(&v.cite.act)
                    .map(|a| a.url.clone())
                    .unwrap_or_default(),
                note: v.note.clone(),
            })
            .collect();
        RulePackDto {
            provenance: self.pack.provenance(),
            review: format!("{:?}", self.pack.info.review).to_lowercase(),
            summary: self.pack.info.summary.clone(),
            omitted: self.pack.info.omitted.clone(),
            values,
            holidays: u32::try_from(self.pack.holidays.len()).unwrap_or(u32::MAX),
        }
    }
}
