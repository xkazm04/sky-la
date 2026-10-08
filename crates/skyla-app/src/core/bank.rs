//! The bank workbench (WP-19): imported statements, the matcher's
//! suggestions, and the postings a person accepts.
//!
//! Imports are tied out against the previous statement (or the books) and
//! deduplicated before anything is kept. Every accepted line becomes one
//! journal entry through the kernel, approved by the user: a receipt that
//! settles issued invoices, a payment that settles received ones, or a
//! booking to accounts (VAT split from the gross with the pack). Rules and
//! the matcher only propose (invariant I7).

use std::collections::{HashMap, HashSet};

use skyla_bank::{
    Action, Condition, Direction, Normalised, OpenItem, Policy, Proposal, Rule, Side, Statement,
    Suggestion, normalise, suggest, tie_out,
};
use skyla_ledger::{
    NewEntry, NewLine, SourceKind, create_draft_as, link_settlement, list_posted, post_entry_at,
};
use skyla_money::{Money, vat};

use super::{Core, money};
use crate::dto::{
    AccountChoiceDto, BankAllocationDto, BankImportDto, BankLineDto, BankRuleDto, BankRuleInputDto,
    BankStatementDto, MatchCandidateDto, ScoreContributionDto, VatCodeChoiceDto,
};
use crate::error::CoreError;

const RECEIVABLES: &str = "311";
const PAYABLES: &str = "321";
const INPUT_VAT: &str = "343";

/// The namespace of bank postings' ids (UUID v5 of the line id).
const BANK_NAMESPACE: uuid::Uuid = uuid::uuid!("8a3c1d7e-2b6f-5e94-a1c0-3d5f7b9e2c46");

pub(crate) struct Imported {
    seq: u32,
    file: String,
    statement: Statement,
    lines: Vec<Normalised>,
}

pub(crate) struct Booked {
    entry_id: i64,
    label: String,
}

/// What the workbench holds between commands.
#[derive(Default)]
pub(crate) struct BankState {
    imports: Vec<Imported>,
    rules: Vec<Rule>,
    booked: HashMap<String, Booked>,
    known: HashSet<String>,
    /// Accounts each customer or supplier has paid from or to.
    payers: HashMap<String, Vec<String>>,
}

fn line_id(seq: u32, l: &Normalised) -> String {
    format!("s{seq}-{}", l.line.sequence)
}

fn bad(message: impl Into<String>) -> CoreError {
    CoreError::BadRequest(message.into())
}

impl Core {
    /// The ledger account the statements are for.
    fn bank_account(&self) -> &str {
        &self.domain.entity.bank_account
    }

    fn bank_state(&self) -> std::sync::MutexGuard<'_, BankState> {
        self.bank
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Reads a statement file, ties it out against the previous statement
    /// (or the books), drops lines already imported, and keeps it.
    pub fn import_bank_statement(
        &self,
        file_name: &str,
        content_base64: &str,
    ) -> Result<BankStatementDto, CoreError> {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(content_base64.trim())
            .map_err(|_| bad("the file isn't base64"))?;
        self.import_bytes(file_name, &bytes)?;
        self.bank_statement()
    }

    pub(crate) fn import_bytes(&self, file_name: &str, bytes: &[u8]) -> Result<(), CoreError> {
        let statements = skyla_bank::parse(bytes, None).map_err(|e| bad(e.to_string()))?;
        let mut state = self.bank_state();
        for statement in statements {
            let iban = statement.account.iban.clone().unwrap_or_default();
            // A file seen before says so, before any balance check.
            let lines = normalise(&iban, &statement.lines);
            if !lines.is_empty() && lines.iter().all(|l| state.known.contains(&l.key)) {
                return Err(bad(format!(
                    "every line in {file_name} was imported before"
                )));
            }
            let expected = match state.imports.last() {
                Some(prev) => prev.statement.closing,
                None => {
                    let before = statement
                        .from
                        .as_deref()
                        .and_then(skyla_rules::date::parse)
                        .map(|d| skyla_rules::date::format(d - 1))
                        .ok_or_else(|| bad("the statement doesn't say which days it covers"))?;
                    Some(self.account_balance(self.bank_account(), &before)?)
                }
            };
            tie_out(&statement, expected).map_err(|e| bad(e.to_string()))?;
            let fresh: Vec<Normalised> = lines
                .into_iter()
                .filter(|l| !state.known.contains(&l.key))
                .collect();
            if fresh.is_empty() {
                return Err(bad(format!(
                    "every line in {file_name} was imported before"
                )));
            }
            for l in &fresh {
                state.known.insert(l.key.clone());
            }
            let seq = u32::try_from(state.imports.len() + 1).unwrap_or(u32::MAX);
            state.imports.push(Imported {
                seq,
                file: file_name.to_owned(),
                statement,
                lines: fresh,
            });
        }
        Ok(())
    }

    fn account_balance(&self, code: &str, as_of: &str) -> Result<Money, CoreError> {
        Ok(skyla_ledger::trial_balance(&self.db(), None, as_of)?
            .rows
            .into_iter()
            .find(|r| r.code == code)
            .map_or(Money::zero(self.currency), |r| r.balance))
    }

    /// Issued and received invoices still open, as the matcher sees them.
    /// Ids are the documents' ledger entries.
    fn open_items(&self, state: &BankState) -> Result<Vec<OpenItem>, CoreError> {
        let mut items = Vec::new();
        for i in self.invoices()? {
            let (Some(number), Some(entry_id)) = (i.number.clone(), i.entry_id) else {
                continue;
            };
            if i.open.minor <= 0 {
                continue;
            }
            items.push(OpenItem {
                side: Side::Receivable,
                id: entry_id,
                vs: skyla_invoicing::spayd::variable_symbol(&number),
                number,
                known_accounts: state.payers.get(&i.client).cloned().unwrap_or_default(),
                customer: i.client,
                issue_date: i.issued_on.unwrap_or_default(),
                due_date: i.due_on,
                open: Money::new(i.open.minor, self.currency),
            });
        }
        // Received invoices: entries crediting payables, less what settled them.
        let db = self.db();
        for e in list_posted(&db, "2000-01-01", "2999-12-31")? {
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
            let paid: i64 = skyla_ledger::settlements_of(&db, e.id)?
                .iter()
                .map(|s| s.amount.minor())
                .sum();
            if owed - paid <= 0 {
                continue;
            }
            let number = e.source_ref.clone().unwrap_or_else(|| format!("#{}", e.id));
            let supplier = e
                .memo
                .split_once(number.as_str())
                .map_or(e.memo.as_str(), |(_, rest)| rest)
                .trim()
                .to_owned();
            items.push(OpenItem {
                side: Side::Payable,
                id: e.id,
                vs: Some(
                    number
                        .chars()
                        .filter(char::is_ascii_digit)
                        .collect::<String>()
                        .trim_start_matches('0')
                        .to_owned(),
                ),
                known_accounts: state.payers.get(&supplier).cloned().unwrap_or_default(),
                customer: supplier,
                number,
                issue_date: e.date.clone(),
                due_date: None,
                open: Money::new(owed - paid, self.currency),
            });
        }
        Ok(items)
    }

    fn suggestions(&self, state: &BankState) -> Result<HashMap<String, Suggestion>, CoreError> {
        let items = self.open_items(state)?;
        let mut out = HashMap::new();
        for imp in &state.imports {
            for l in &imp.lines {
                let id = line_id(imp.seq, l);
                if !state.booked.contains_key(&id) {
                    out.insert(id, suggest(l, &items, &state.rules, &Policy::default()));
                }
            }
        }
        Ok(out)
    }

    fn find_line<'a>(state: &'a BankState, id: &str) -> Result<&'a Normalised, CoreError> {
        state
            .imports
            .iter()
            .flat_map(|imp| imp.lines.iter().map(move |l| (line_id(imp.seq, l), l)))
            .find(|(lid, _)| lid == id)
            .map(|(_, l)| l)
            .ok_or_else(|| bad(format!("no bank line {id}")))
    }

    /// Posts one entry for a line and records it as booked.
    fn post_line(
        &self,
        state: &mut BankState,
        id: &str,
        lines: Vec<NewLine>,
        settles: &[(i64, Money)],
        created_by: &str,
        label: String,
    ) -> Result<(), CoreError> {
        let line = Self::find_line(state, id)?.clone();
        if state.booked.contains_key(id) {
            return Err(bad(format!("bank line {id} is booked already")));
        }
        let memo = [
            line.line.counterparty_name.as_deref(),
            line.line.message.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        let entry = NewEntry {
            date: line.line.booking_date.clone(),
            source_kind: SourceKind::Bank,
            source_ref: line.line.bank_ref.clone().or_else(|| Some(id.to_owned())),
            memo: if memo.is_empty() {
                "Bankovní pohyb".into()
            } else {
                memo
            },
            created_by: created_by.to_owned(),
            lines,
        };
        let uid = uuid::Uuid::new_v5(&BANK_NAMESPACE, id.as_bytes()).to_string();
        let posted_at = format!("{}T12:00:00.000Z", line.line.booking_date);
        let db = self.db();
        db.execute_batch("SAVEPOINT bank_line")
            .map_err(|e| bad(e.to_string()))?;
        let result = (|| -> Result<i64, CoreError> {
            let entry_id = create_draft_as(&db, &entry, &uid)?;
            for (settled, amount) in settles {
                link_settlement(&db, entry_id, *settled, *amount)?;
            }
            post_entry_at(&db, entry_id, Some("user"), Some(&posted_at))?;
            Ok(entry_id)
        })();
        match result {
            Ok(entry_id) => {
                db.execute_batch("RELEASE bank_line")
                    .map_err(|e| bad(e.to_string()))?;
                state
                    .booked
                    .insert(id.to_owned(), Booked { entry_id, label });
                Ok(())
            }
            Err(e) => {
                let _ = db.execute_batch("ROLLBACK TO bank_line; RELEASE bank_line");
                Err(e)
            }
        }
    }

    /// Settles invoices with a line: a receipt against 311, or a payment against 321.
    fn settle_line(
        &self,
        state: &mut BankState,
        id: &str,
        allocations: &[(i64, Money)],
        by: &str,
    ) -> Result<(), CoreError> {
        let line = Self::find_line(state, id)?.clone();
        let items = self.open_items(state)?;
        let total: i64 = allocations.iter().map(|(_, m)| m.minor()).sum();
        if total != line.line.amount.minor().abs() {
            return Err(bad("the allocations don't add up to the line"));
        }
        let incoming = line.line.amount.minor() > 0;
        let mut entry_lines = Vec::new();
        let mut numbers = Vec::new();
        let mut party = None;
        for (entry_id, amount) in allocations {
            let item = items
                .iter()
                .find(|i| i.id == *entry_id && (i.side == Side::Receivable) == incoming)
                .ok_or_else(|| bad(format!("entry {entry_id} isn't open on this side")))?;
            if amount.minor() > item.open.minor() {
                return Err(bad(format!(
                    "{} has only {} open",
                    item.number,
                    item.open.format_cs()
                )));
            }
            numbers.push(item.number.clone());
            party = Some(item.customer.clone());
            entry_lines.push(if incoming {
                NewLine::credit(RECEIVABLES, *amount)?
            } else {
                NewLine::debit(PAYABLES, *amount)
            });
        }
        let gross = Money::new(total, self.currency);
        entry_lines.insert(
            0,
            if incoming {
                NewLine::debit(self.bank_account(), gross)
            } else {
                NewLine::credit(self.bank_account(), gross)?
            },
        );
        let label = format!(
            "{} {}",
            if incoming { "Settles" } else { "Pays" },
            numbers.join(" + ")
        );
        self.post_line(state, id, entry_lines, allocations, by, label)?;
        if let (Some(party), Some(account)) = (party, line.counterparty_account.clone()) {
            let known = state.payers.entry(party).or_default();
            if !known.contains(&account) {
                known.push(account);
            }
        }
        Ok(())
    }

    /// Books a line to accounts. Each row is a gross amount; a VAT code
    /// splits it into base and input VAT with the pack's rate on the day.
    fn book_rows(
        &self,
        state: &mut BankState,
        id: &str,
        rows: &[(String, Option<String>, Money)],
        by: &str,
        label: String,
    ) -> Result<(), CoreError> {
        let line = Self::find_line(state, id)?.clone();
        let total: i64 = rows.iter().map(|(_, _, m)| m.minor()).sum();
        if total != line.line.amount.minor().abs() || rows.iter().any(|(_, _, m)| m.minor() <= 0) {
            return Err(bad(format!(
                "the rows add up to {}; the line is {}",
                Money::new(total, self.currency).format_cs(),
                Money::new(line.line.amount.minor().abs(), self.currency).format_cs()
            )));
        }
        let out = line.line.amount.minor() < 0;
        let on = line.line.booking_date.as_str();
        let rounding = self.pack.rounding("vat.rounding.document", on)?;
        let mut entry_lines = Vec::new();
        for (account, vat_code, gross) in rows {
            let sign = |m: Money| -> Result<NewLine, CoreError> {
                Ok(if out {
                    NewLine::debit(account, m)
                } else {
                    NewLine::credit(account, m)?
                })
            };
            match vat_code {
                Some(code) => {
                    if !out || !code.starts_with("IN") {
                        return Err(bad(format!(
                            "VAT code {code} isn't an input code for a purchase"
                        )));
                    }
                    let rate = self.pack.vat_rate(code, on)?;
                    let split = vat::from_gross(*gross, rate, rounding)?;
                    let mut base = sign(split.base)?;
                    base.vat_code = Some(code.clone());
                    let mut tax = NewLine::debit(INPUT_VAT, split.vat);
                    tax.vat_code = Some(code.clone());
                    entry_lines.push(base);
                    if split.vat.minor() != 0 {
                        entry_lines.push(tax);
                    }
                }
                None => entry_lines.push(sign(*gross)?),
            }
        }
        let gross = Money::new(total, self.currency);
        entry_lines.push(if out {
            NewLine::credit(self.bank_account(), gross)?
        } else {
            NewLine::debit(self.bank_account(), gross)
        });
        self.post_line(state, id, entry_lines, &[], by, label)
    }

    fn apply(&self, state: &mut BankState, id: &str, s: &Suggestion) -> Result<(), CoreError> {
        match &s.proposal {
            Proposal::Settle { best } => self.settle_line(state, id, &best.allocations, "matcher"),
            Proposal::Rule {
                name,
                action: Action::Book {
                    account, vat_code, ..
                },
                ..
            } => {
                let line = Self::find_line(state, id)?;
                let gross = Money::new(line.line.amount.minor().abs(), self.currency);
                self.book_rows(
                    state,
                    id,
                    &[(account.clone(), vat_code.clone(), gross)],
                    "rule",
                    format!("Rule: {name}"),
                )
            }
            Proposal::Rule { .. } | Proposal::Unmatched => {
                Err(bad(format!("bank line {id} has nothing certain to accept")))
            }
        }
    }

    /// Accepts every line the matcher or a rule is certain about. The user
    /// pressing the button is the approval.
    pub fn accept_certain_bank_lines(&self) -> Result<BankStatementDto, CoreError> {
        {
            let mut state = self.bank_state();
            let suggestions = self.suggestions(&state)?;
            let mut ids: Vec<&String> = suggestions
                .iter()
                .filter(|(_, s)| s.auto)
                .map(|(id, _)| id)
                .collect();
            ids.sort();
            for id in ids {
                if let Some(s) = suggestions.get(id) {
                    self.apply(&mut state, id, s)?;
                }
            }
        }
        self.bank_statement()
    }

    /// Books a line the way the user chose: invoices to settle, or account
    /// rows (a split). Amounts are typed in Czech format.
    pub fn book_bank_line(
        &self,
        line: &str,
        allocations: &[BankAllocationDto],
    ) -> Result<BankStatementDto, CoreError> {
        {
            let mut state = self.bank_state();
            let parse = |a: &BankAllocationDto| {
                skyla_money::parse_amount_cs(&a.amount, self.currency)
                    .map_err(|_| bad(format!("{:?} isn't an amount like 1 200,00", a.amount)))
            };
            if allocations.is_empty() {
                return Err(bad("choose at least one invoice or account"));
            }
            if allocations.iter().all(|a| a.entry_id.is_some()) {
                let picked = allocations
                    .iter()
                    .map(|a| Ok((a.entry_id.unwrap_or_default(), parse(a)?)))
                    .collect::<Result<Vec<_>, CoreError>>()?;
                self.settle_line(&mut state, line, &picked, "user")?;
            } else if allocations.iter().all(|a| a.account.is_some()) {
                let rows = allocations
                    .iter()
                    .map(|a| {
                        Ok((
                            a.account.clone().unwrap_or_default(),
                            a.vat_code.clone(),
                            parse(a)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, CoreError>>()?;
                let label = if rows.len() > 1 {
                    format!(
                        "Split across {}",
                        rows.iter()
                            .map(|r| r.0.as_str())
                            .collect::<Vec<_>>()
                            .join(" + ")
                    )
                } else {
                    format!(
                        "Booked to {}",
                        rows.first().map(|r| r.0.as_str()).unwrap_or_default()
                    )
                };
                self.book_rows(&mut state, line, &rows, "user", label)?;
            } else {
                return Err(bad(
                    "a line settles invoices or books to accounts, not both",
                ));
            }
        }
        self.bank_statement()
    }

    /// Makes a rule from a line (its payee's account, else its name or
    /// message) and books the line by it; other lines it fits are proposed.
    pub fn create_bank_rule(
        &self,
        line: &str,
        input: &BankRuleInputDto,
    ) -> Result<BankStatementDto, CoreError> {
        {
            let mut state = self.bank_state();
            let l = Self::find_line(&state, line)?.clone();
            if input.name.trim().is_empty() {
                return Err(bad("a rule needs a name"));
            }
            if !self.accounts.contains_key(&input.account) {
                return Err(bad(format!("account {} isn't in the chart", input.account)));
            }
            let direction = if l.line.amount.minor() < 0 {
                Direction::Out
            } else {
                Direction::In
            };
            let mut when = vec![Condition::Direction(direction)];
            if let Some(account) = &l.counterparty_account {
                when.insert(0, Condition::CounterpartyAccount(account.clone()));
            } else if !l.counterparty_name.is_empty() {
                when.insert(0, Condition::NameContains(l.counterparty_name.clone()));
            } else if let Some(m) = &l.line.message {
                when.insert(0, Condition::MessageContains(m.clone()));
            } else {
                return Err(bad("the line has nothing a rule could recognise it by"));
            }
            let id = u32::try_from(state.rules.len() + 1).unwrap_or(u32::MAX);
            let rule = Rule {
                id,
                name: input.name.trim().to_owned(),
                when,
                then: Action::Book {
                    account: input.account.clone(),
                    vat_code: input.vat_code.clone(),
                    memo: input.name.trim().to_owned(),
                },
                auto_accept: input.auto_accept,
            };
            state.rules.push(rule.clone());
            let gross = Money::new(l.line.amount.minor().abs(), self.currency);
            self.book_rows(
                &mut state,
                line,
                &[(input.account.clone(), input.vat_code.clone(), gross)],
                "user",
                format!("Rule: {}", rule.name),
            )?;
        }
        self.bank_statement()
    }

    /// The workbench: the latest statement's tie-out, every imported line
    /// with its suggestion or booking, and the rules.
    pub fn bank_statement(&self) -> Result<BankStatementDto, CoreError> {
        let state = self.bank_state();
        let suggestions = self.suggestions(&state)?;
        let latest = state
            .imports
            .last()
            .ok_or_else(|| bad("no statement imported yet"))?;
        let tie = tie_out(&latest.statement, None).map_err(|e| bad(e.to_string()))?;
        let mut lines = Vec::new();
        for imp in state.imports.iter().rev() {
            for l in &imp.lines {
                let id = line_id(imp.seq, l);
                let reference = [
                    l.line.vs.as_ref().map(|v| format!("VS {v}")),
                    l.line.message.clone(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                let booked = state.booked.get(&id);
                let s = suggestions.get(&id);
                let candidates = s
                    .map(|s| s.candidates.iter().take(3).map(candidate_dto).collect())
                    .unwrap_or_default();
                let (status, proposal_label) = match (booked, s) {
                    (Some(_), _) => ("booked", None),
                    (None, Some(s)) => (
                        if s.auto { "certain" } else { "needs_you" },
                        match &s.proposal {
                            Proposal::Settle { best } => Some(format!(
                                "{} {}",
                                if l.line.amount.minor() > 0 {
                                    "Settle"
                                } else {
                                    "Pay"
                                },
                                best.numbers.join(" + ")
                            )),
                            Proposal::Rule { name, .. } => Some(format!("Rule: {name}")),
                            Proposal::Unmatched => None,
                        },
                    ),
                    (None, None) => ("needs_you", None),
                };
                lines.push(BankLineDto {
                    id: id.clone(),
                    date: l.line.booking_date.clone(),
                    counterparty: l
                        .line
                        .counterparty_name
                        .clone()
                        .or_else(|| l.line.message.clone())
                        .unwrap_or_else(|| "Bank".into()),
                    counterparty_account: l.counterparty_account.clone(),
                    reference,
                    amount: money(l.line.amount)?,
                    status: status.into(),
                    booked_as: booked.map(|b| b.label.clone()),
                    entry_id: booked.map(|b| b.entry_id),
                    proposal: proposal_label,
                    held_because: s
                        .and_then(|s| s.held_because.clone())
                        .filter(|_| booked.is_none()),
                    proposal_id: self.proposal_for_line(&id),
                    candidates,
                });
            }
        }
        lines.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
        let statement = &latest.statement;
        Ok(BankStatementDto {
            account_name: format!("{} · {}", self.bank_account(), self.domain.entity.bank_name),
            file: latest.file.clone(),
            format: "CAMT.053".into(),
            from: statement.from.clone().unwrap_or_default(),
            to: statement.to.clone().unwrap_or_default(),
            opening: money(tie.opening)?,
            credits: money(tie.credits)?,
            debits: money(tie.debits)?,
            closing: money(tie.closing)?,
            reported_closing: money(tie.closing)?,
            ties_out: true,
            imports: state
                .imports
                .iter()
                .map(|imp| {
                    Ok(BankImportDto {
                        file: imp.file.clone(),
                        from: imp.statement.from.clone().unwrap_or_default(),
                        to: imp.statement.to.clone().unwrap_or_default(),
                        lines: u32::try_from(imp.lines.len()).unwrap_or(u32::MAX),
                        closing: imp.statement.closing.map(money).transpose()?,
                    })
                })
                .collect::<Result<_, CoreError>>()?,
            rules: state
                .rules
                .iter()
                .map(|r| BankRuleDto {
                    id: r.id,
                    name: r.name.clone(),
                    summary: rule_summary(r),
                    auto_accept: r.auto_accept,
                })
                .collect(),
            accounts: self.bookable_accounts(),
            vat_codes: self
                .pack
                .vat_codes
                .iter()
                .filter(|c| c.code.starts_with("IN"))
                .map(|c| {
                    Ok(VatCodeChoiceDto {
                        code: c.code.clone(),
                        name: c.name.clone(),
                        rate_percent: self
                            .pack
                            .vat_rate(&c.code, &statement.to.clone().unwrap_or_default())?
                            .normalize()
                            .to_string(),
                    })
                })
                .collect::<Result<_, CoreError>>()?,
            lines,
        })
    }

    /// Cash, payables and the expense and revenue accounts, by code.
    fn bookable_accounts(&self) -> Vec<AccountChoiceDto> {
        let mut out: Vec<AccountChoiceDto> = self
            .accounts
            .iter()
            .filter(|(code, _)| {
                code.len() == 3
                    && (code.starts_with('5')
                        || code.starts_with('6')
                        || ["211", "321", "335"].contains(&code.as_str()))
            })
            .map(|(code, (name, _))| AccountChoiceDto {
                code: code.clone(),
                name: name.clone(),
            })
            .collect();
        out.sort_by(|a, b| a.code.cmp(&b.code));
        out
    }
}

fn candidate_dto(c: &skyla_bank::Candidate) -> MatchCandidateDto {
    MatchCandidateDto {
        label: c.numbers.join(" + "),
        detail: c
            .allocations
            .iter()
            .map(|(_, m)| m.format_cs())
            .collect::<Vec<_>>()
            .join(" + "),
        score: c.score.to_string(),
        confidence: if c.score >= Policy::default().threshold {
            "likely".into()
        } else {
            "unlikely".into()
        },
        contributions: c
            .contributions
            .iter()
            .map(|x| ScoreContributionDto {
                reason: x.detail.clone(),
                weight: if x.points >= 0 {
                    format!("+{}", x.points)
                } else {
                    x.points.to_string()
                },
            })
            .collect(),
    }
}

fn rule_summary(r: &Rule) -> String {
    let when: Vec<String> = r
        .when
        .iter()
        .map(|c| match c {
            Condition::CounterpartyAccount(a) => format!("to or from {a}"),
            Condition::NameContains(n) => format!("name has \"{n}\""),
            Condition::MessageContains(m) => format!("message has \"{m}\""),
            Condition::Vs(v) => format!("VS {v}"),
            Condition::Ks(k) => format!("KS {k}"),
            Condition::Direction(Direction::In) => "money in".into(),
            Condition::Direction(Direction::Out) => "money out".into(),
            Condition::AmountBetween { min, max } => format!("{min}–{max}"),
        })
        .collect();
    let then = match &r.then {
        Action::Book {
            account, vat_code, ..
        } => match vat_code {
            Some(v) => format!("book to {account} with {v}"),
            None => format!("book to {account}"),
        },
        Action::SettleCustomer { customer } => format!("settle {customer}'s invoices"),
    };
    format!("{} → {then}", when.join(", "))
}
