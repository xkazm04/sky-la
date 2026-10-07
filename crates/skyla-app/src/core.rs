use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;
use skyla_ledger::{
    AccountKind, balance_sheet, cash_basis, functional_currency, list_accounts, list_periods,
    list_posted, profit_and_loss, trial_balance, verify_chain,
};
use skyla_money::{Currency, Money, vat};
use skyla_rules::{Pack, RowPart};

use crate::CoreError;
use crate::demo::{Domain, DomainEntry, demo_domain, demo_ledger};
use crate::dto::*;

/// Accounts that hold VAT in the chart. A chart property, not a statutory
/// value: CZ books VAT on 343 (and its analytic sub-accounts).
const VAT_ACCOUNTS: &[&str] = &["343"];

/// The application core: one open entity and its ledger.
pub struct Core {
    conn: Mutex<Connection>,
    domain: Domain,
    accounts: HashMap<String, (String, AccountKind)>,
    currency: Currency,
    pack: Pack,
    /// Scheduled drafts' issue dates (recurring invoices arrive in WP-15).
    scheduled: HashMap<i64, String>,
}

fn money(m: Money) -> Result<MoneyDto, CoreError> {
    MoneyDto::try_from(m)
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
    /// Opens the demo entity (WP-09). The unlock flow (WP-30) adds `Core::open`.
    pub fn demo() -> Result<Self, CoreError> {
        let conn = demo_ledger()?;
        let currency = functional_currency(&conn)?;
        let accounts = list_accounts(&conn)?
            .into_iter()
            .map(|a| (a.code, (a.name_en, a.kind)))
            .collect();
        let domain = demo_domain()?;
        let pack = Pack::cz_2026()?;
        let scheduled = crate::demo::seed_invoicing(&conn, &pack, &domain)?;
        Ok(Self {
            conn: Mutex::new(conn),
            domain,
            accounts,
            currency,
            pack,
            scheduled,
        })
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

    /// Writes an issued document as ISDOC 6.0.2, with the document it refers
    /// to and the advances it deducts.
    pub fn invoice_isdoc(&self, id: i64) -> Result<DocumentXmlDto, CoreError> {
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
        let xml = skyla_invoicing::to_isdoc(
            &self.pack,
            skyla_invoicing::ExportInput {
                doc: &doc,
                related: related.as_ref(),
                advances: &advances,
            },
        )?;
        let number = doc.number.unwrap_or_else(|| format!("draft-{id}"));
        Ok(DocumentXmlDto {
            file_name: format!("{number}.isdoc"),
            media_type: "application/xml".into(),
            xml,
        })
    }

    /// The latest bank import, tied out against the ledger.
    pub fn bank_statement(&self) -> Result<BankStatementDto, CoreError> {
        let import = &self.domain.bank_import;
        let account = &self.domain.entity.bank_account;
        let opening = trial_balance(&self.db(), None, &import.opening_as_of)?
            .rows
            .into_iter()
            .find(|r| &r.code == account)
            .map_or(Money::zero(self.currency), |r| r.balance);
        let (mut credits, mut debits) = (0_i64, 0_i64);
        let mut lines = Vec::new();
        for l in &import.lines {
            if l.amount_minor >= 0 {
                credits = credits
                    .checked_add(l.amount_minor)
                    .ok_or(skyla_money::MoneyError::Overflow)?;
            } else {
                debits = debits
                    .checked_add(l.amount_minor)
                    .ok_or(skyla_money::MoneyError::Overflow)?;
            }
            lines.push(BankLineDto {
                id: l.id.clone(),
                date: l.date.clone(),
                counterparty: l.counterparty.clone(),
                reference: l.reference.clone(),
                amount: self.amount(l.amount_minor)?,
                foreign: l
                    .foreign
                    .as_ref()
                    .map(|f| -> Result<ForeignAmountDto, CoreError> {
                        Ok(ForeignAmountDto {
                            amount: money(Money::new(
                                f.amount_minor,
                                Currency::from_code(&f.currency)?,
                            ))?,
                            rate: f.rate.clone(),
                        })
                    })
                    .transpose()?,
                status: l.status.clone(),
                matched_to: l.matched_to.clone(),
                proposal_id: l.proposal_id.clone(),
                candidates: l.candidates.clone(),
            });
        }
        lines.sort_by(|a, b| b.date.cmp(&a.date));
        let closing = opening
            .checked_add(Money::new(credits, self.currency))?
            .checked_add(Money::new(debits, self.currency))?;
        let reported = Money::new(import.reported_closing_minor, self.currency);
        Ok(BankStatementDto {
            account_name: format!("{account} · {}", self.domain.entity.bank_name),
            file: import.file.clone(),
            format: import.format.clone(),
            from: import.from.clone(),
            to: import.to.clone(),
            opening: money(opening)?,
            credits: self.amount(credits)?,
            debits: self.amount(debits)?,
            closing: money(closing)?,
            reported_closing: money(reported)?,
            ties_out: closing == reported,
            lines,
        })
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

    /// The inbox: proposals waiting for a human decision.
    pub fn proposals(&self) -> Result<Vec<ProposalDto>, CoreError> {
        let open_invoices: HashMap<String, i64> = self
            .invoices()?
            .into_iter()
            .filter_map(|i| i.number.map(|n| (n, i.open.minor)))
            .collect();
        self.domain
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
                    .and_then(|id| self.domain.bank_import.lines.iter().find(|l| &l.id == id))
                    .map(|l| self.amount(l.amount_minor))
                    .transpose()?;
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
            .collect()
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

    /// Every advisor run, newest first.
    pub fn egress_register(&self) -> Vec<EgressRunDto> {
        let mut runs = self.domain.egress_runs.clone();
        runs.sort_by(|a, b| b.at.cmp(&a.at));
        runs
    }
}
