//! Projections: pure functions of the posted journal (WP-07).
//!
//! Trial balance, profit and loss, balance sheet, cash basis (*daňová
//! evidence*) and the VAT ledger by form row. Nothing is cached. Every report
//! carries a [`Snapshot`]: a hash over the chain hashes of exactly the posted
//! entries it read, plus its parameters, so the same inputs always give the
//! same snapshot and any change to them gives a different one.

use std::collections::HashMap;

use rusqlite::{Connection, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use skyla_money::{Currency, Money};

use crate::accounts::treatment_from;
use crate::posting::{functional_currency, is_iso_date};
use crate::{AccountKind, Direction, LedgerError, TaxTreatment};

/// What a report read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    /// Posted entries read.
    pub entries: u64,
    /// The highest posting sequence number among them.
    pub last_posted_seq: Option<i64>,
    /// SHA-256 (hex) over the report kind, its parameters and each entry's
    /// sequence number and chain hash, in sequence order.
    pub hash: String,
}

/// Hashes what a report read: its kind and parameters, then each entry's
/// sequence number and chain hash in sequence order.
fn seal(
    kind: &str,
    from: Option<&str>,
    to: &str,
    parameters: &str,
    mut read: Vec<(i64, [u8; 64])>,
) -> Snapshot {
    read.sort_unstable_by_key(|(seq, _)| *seq);
    let mut hasher = Sha256::new();
    for part in [
        "skyla-ledger/report-snapshot/v1",
        kind,
        from.unwrap_or(""),
        to,
        parameters,
    ] {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    for (seq, hash) in &read {
        hasher.update(seq.to_be_bytes());
        hasher.update(hash);
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Snapshot {
        entries: read.len() as u64,
        last_posted_seq: read.last().map(|(seq, _)| *seq),
        hash: crate::chain::to_hex(&digest),
    }
}

fn hash_bytes(value: rusqlite::types::ValueRef<'_>) -> [u8; 64] {
    let mut out = [0; 64];
    if let Ok(text) = value.as_bytes() {
        let n = text.len().min(64);
        out[..n].copy_from_slice(&text[..n]);
    }
    out
}

/// The snapshot of every posted entry dated in the range.
fn snapshot(
    conn: &Connection,
    kind: &str,
    from: Option<&str>,
    to: &str,
    parameters: &str,
) -> Result<Snapshot, LedgerError> {
    let mut stmt = conn.prepare_cached(
        "SELECT posted_seq, chain_hash FROM journal_entry
         WHERE status = 'posted' AND entry_date BETWEEN ?1 AND ?2",
    )?;
    let mut rows = stmt.query(params![from.unwrap_or("0000-01-01"), to])?;
    let mut read = Vec::new();
    while let Some(row) = rows.next()? {
        read.push((row.get(0)?, hash_bytes(row.get_ref(1)?)));
    }
    Ok(seal(kind, from, to, parameters, read))
}

fn check_range(from: Option<&str>, to: &str) -> Result<(), LedgerError> {
    for date in from.into_iter().chain([to]) {
        if !is_iso_date(date) {
            return Err(LedgerError::InvalidDate(date.to_owned()));
        }
    }
    if from.is_some_and(|f| f > to) {
        return Err(LedgerError::InvalidEntry(format!(
            "the report range starts after it ends ({} > {to})",
            from.unwrap_or_default()
        )));
    }
    Ok(())
}

fn kind_of(text: &str) -> AccountKind {
    match text {
        "asset" => AccountKind::Asset,
        "liability" => AccountKind::Liability,
        "equity" => AccountKind::Equity,
        "revenue" => AccountKind::Revenue,
        "expense" => AccountKind::Expense,
        _ => AccountKind::Closing,
    }
}

// ------------------------------------------------------------ trial balance

/// One account's movement in a trial balance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrialBalanceRow {
    /// Leaf account code.
    pub code: String,
    /// Czech name.
    pub name_cs: String,
    /// English name.
    pub name_en: String,
    /// Account kind.
    pub kind: AccountKind,
    /// Sum of debits.
    pub debit: Money,
    /// Sum of credits, as a positive amount.
    pub credit: Money,
    /// Debit minus credit.
    pub balance: Money,
}

/// Debits and credits per account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrialBalance {
    /// First day, or `None` for everything up to `to`.
    pub from: Option<String>,
    /// Last day, inclusive.
    pub to: String,
    /// One row per account with postings, by code.
    pub rows: Vec<TrialBalanceRow>,
    /// Total debits.
    pub total_debit: Money,
    /// Total credits (positive).
    pub total_credit: Money,
    /// What it read.
    pub snapshot: Snapshot,
}

impl TrialBalance {
    /// True when debits equal credits, which every posted journal guarantees.
    pub fn balances(&self) -> bool {
        self.total_debit == self.total_credit
    }
}

struct Movement {
    code: String,
    name_cs: String,
    name_en: String,
    kind: AccountKind,
    debit: i64,
    credit: i64,
}

/// Per-account movements for entries dated in the range, grouped by leaf
/// (`synthetic = false`) or by three-digit synthetic account, and the
/// snapshot of what was read. One streaming pass over covering indexes, so a
/// trial balance over 100 000 entries stays well under 200 ms.
fn movements(
    conn: &Connection,
    kind: &str,
    from: Option<&str>,
    to: &str,
    synthetic: bool,
) -> Result<(Vec<Movement>, Snapshot), LedgerError> {
    let mut totals: HashMap<i64, (i64, i64)> = HashMap::new();
    let mut read: Vec<(i64, [u8; 64])> = Vec::new();
    let mut stmt = conn.prepare_cached(
        "SELECT e.posted_seq, e.chain_hash, p.account_id, p.amount_func_minor
         FROM journal_entry e JOIN posting p ON p.entry_id = e.id
         WHERE e.status = 'posted' AND e.entry_date BETWEEN ?1 AND ?2",
    )?;
    let mut rows = stmt.query(params![from.unwrap_or("0000-01-01"), to])?;
    let overflow = || LedgerError::from(skyla_money::MoneyError::Overflow);
    while let Some(row) = rows.next()? {
        let seq: i64 = row.get(0)?;
        if read.last().is_none_or(|(last, _)| *last != seq) {
            read.push((seq, hash_bytes(row.get_ref(1)?)));
        }
        let amount: i64 = row.get(3)?;
        let total = totals.entry(row.get(2)?).or_default();
        if amount > 0 {
            total.0 = total.0.checked_add(amount).ok_or_else(overflow)?;
        } else {
            total.1 = total.1.checked_add(-amount).ok_or_else(overflow)?;
        }
    }
    // Rows of one entry arrive together, but an entry could repeat if the
    // planner changes; the seal sorts, so drop exact duplicates.
    read.sort_unstable_by_key(|(seq, _)| *seq);
    read.dedup_by_key(|(seq, _)| *seq);

    let mut accounts: HashMap<i64, (String, String, String, AccountKind)> = HashMap::new();
    let mut by_code: HashMap<String, i64> = HashMap::new();
    let mut stmt = conn.prepare_cached("SELECT id, code, name_cs, name_en, kind FROM account")?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let (id, code): (i64, String) = (r.get(0)?, r.get(1)?);
        by_code.insert(code.clone(), id);
        accounts.insert(
            id,
            (code, r.get(2)?, r.get(3)?, kind_of(&r.get::<_, String>(4)?)),
        );
    }
    let mut grouped: std::collections::BTreeMap<String, (i64, i64)> = Default::default();
    for (id, (debit, credit)) in totals {
        let code = &accounts
            .get(&id)
            .ok_or_else(|| LedgerError::UnknownAccount(format!("id {id}")))?
            .0;
        let key = if synthetic {
            code.get(..3).unwrap_or(code)
        } else {
            code
        };
        let group = grouped.entry(key.to_owned()).or_default();
        group.0 = group.0.checked_add(debit).ok_or_else(overflow)?;
        group.1 = group.1.checked_add(credit).ok_or_else(overflow)?;
    }
    let movements = grouped
        .into_iter()
        .map(|(code, (debit, credit))| {
            let id = by_code.get(&code).copied().unwrap_or_default();
            let (_, name_cs, name_en, kind) = accounts.get(&id).cloned().unwrap_or((
                code.clone(),
                String::new(),
                String::new(),
                AccountKind::Closing,
            ));
            Movement {
                code,
                name_cs,
                name_en,
                kind,
                debit,
                credit,
            }
        })
        .collect();
    Ok((movements, seal(kind, from, to, "", read)))
}

/// Debits, credits and balance per leaf account for entries dated `from..=to`
/// (`from = None`: from the beginning).
pub fn trial_balance(
    conn: &Connection,
    from: Option<&str>,
    to: &str,
) -> Result<TrialBalance, LedgerError> {
    check_range(from, to)?;
    let currency = functional_currency(conn)?;
    let money = |minor| Money::new(minor, currency);
    let mut total_debit = 0_i64;
    let mut total_credit = 0_i64;
    let (movements, snapshot) = movements(conn, "trial_balance", from, to, false)?;
    let rows = movements
        .into_iter()
        .map(|m| {
            total_debit = total_debit
                .checked_add(m.debit)
                .ok_or(skyla_money::MoneyError::Overflow)?;
            total_credit = total_credit
                .checked_add(m.credit)
                .ok_or(skyla_money::MoneyError::Overflow)?;
            Ok(TrialBalanceRow {
                balance: money(
                    m.debit
                        .checked_sub(m.credit)
                        .ok_or(skyla_money::MoneyError::Overflow)?,
                ),
                debit: money(m.debit),
                credit: money(m.credit),
                code: m.code,
                name_cs: m.name_cs,
                name_en: m.name_en,
                kind: m.kind,
            })
        })
        .collect::<Result<_, LedgerError>>()?;
    Ok(TrialBalance {
        from: from.map(str::to_owned),
        to: to.to_owned(),
        rows,
        total_debit: money(total_debit),
        total_credit: money(total_credit),
        snapshot,
    })
}

// ------------------------------------------------- statements (P&L and BS)

/// One line of a statement, by three-digit synthetic account. Amounts are
/// shown the way the statement reads: revenue, expenses, assets, liabilities
/// and equity all positive in their normal state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatementLine {
    /// Synthetic account code.
    pub code: String,
    /// Czech name.
    pub name_cs: String,
    /// English name.
    pub name_en: String,
    /// The amount.
    pub amount: Money,
}

/// Profit and loss for a date range, accrual basis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfitAndLoss {
    /// First day.
    pub from: String,
    /// Last day, inclusive.
    pub to: String,
    /// Revenue lines (credit-positive).
    pub revenue: Vec<StatementLine>,
    /// Expense lines (debit-positive).
    pub expenses: Vec<StatementLine>,
    /// Sum of revenue.
    pub total_revenue: Money,
    /// Sum of expenses.
    pub total_expenses: Money,
    /// Revenue minus expenses.
    pub profit: Money,
    /// What it read.
    pub snapshot: Snapshot,
}

fn line(m: &Movement, amount: Money) -> StatementLine {
    StatementLine {
        code: m.code.clone(),
        name_cs: m.name_cs.clone(),
        name_en: m.name_en.clone(),
        amount,
    }
}

fn total(currency: Currency, lines: &[StatementLine]) -> Result<Money, LedgerError> {
    Ok(Money::sum(currency, lines.iter().map(|l| l.amount))?)
}

/// Profit and loss for entries dated `from..=to`, by synthetic account.
pub fn profit_and_loss(
    conn: &Connection,
    from: &str,
    to: &str,
) -> Result<ProfitAndLoss, LedgerError> {
    check_range(Some(from), to)?;
    let currency = functional_currency(conn)?;
    let mut revenue = Vec::new();
    let mut expenses = Vec::new();
    let (movements, snapshot) = movements(conn, "profit_and_loss", Some(from), to, true)?;
    for m in movements {
        let debit_balance = m.debit - m.credit;
        match m.kind {
            AccountKind::Revenue if debit_balance != 0 => {
                revenue.push(line(&m, Money::new(-debit_balance, currency)))
            }
            AccountKind::Expense if debit_balance != 0 => {
                expenses.push(line(&m, Money::new(debit_balance, currency)))
            }
            _ => {}
        }
    }
    let total_revenue = total(currency, &revenue)?;
    let total_expenses = total(currency, &expenses)?;
    Ok(ProfitAndLoss {
        from: from.to_owned(),
        to: to.to_owned(),
        profit: total_revenue.checked_sub(total_expenses)?,
        revenue,
        expenses,
        total_revenue,
        total_expenses,
        snapshot,
    })
}

/// Balance sheet as of a date: everything posted up to and including it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BalanceSheet {
    /// The date.
    pub as_of: String,
    /// Asset lines; contra accounts (accumulated depreciation) are negative.
    pub assets: Vec<StatementLine>,
    /// Liability lines.
    pub liabilities: Vec<StatementLine>,
    /// Equity lines, including any balance left on closing accounts.
    pub equity: Vec<StatementLine>,
    /// Revenue minus expenses not yet closed into equity.
    pub unclosed_profit: Money,
    /// Sum of assets.
    pub total_assets: Money,
    /// Sum of liabilities.
    pub total_liabilities: Money,
    /// Sum of equity lines plus the unclosed profit.
    pub total_equity: Money,
    /// What it read.
    pub snapshot: Snapshot,
}

impl BalanceSheet {
    /// Assets equal liabilities plus equity, which a balanced journal guarantees.
    pub fn balances(&self) -> bool {
        self.total_liabilities
            .checked_add(self.total_equity)
            .is_ok_and(|sum| sum == self.total_assets)
    }
}

/// Balance sheet as of `as_of`, by synthetic account.
pub fn balance_sheet(conn: &Connection, as_of: &str) -> Result<BalanceSheet, LedgerError> {
    check_range(None, as_of)?;
    let currency = functional_currency(conn)?;
    let (mut assets, mut liabilities, mut equity) = (Vec::new(), Vec::new(), Vec::new());
    let mut profit = 0_i64;
    let (movements, snapshot) = movements(conn, "balance_sheet", None, as_of, true)?;
    for m in movements {
        let debit_balance = m.debit - m.credit;
        match m.kind {
            AccountKind::Revenue | AccountKind::Expense => profit -= debit_balance,
            _ if debit_balance == 0 => {}
            AccountKind::Asset => assets.push(line(&m, Money::new(debit_balance, currency))),
            AccountKind::Liability => {
                liabilities.push(line(&m, Money::new(-debit_balance, currency)))
            }
            AccountKind::Equity | AccountKind::Closing => {
                equity.push(line(&m, Money::new(-debit_balance, currency)))
            }
        }
    }
    let unclosed_profit = Money::new(profit, currency);
    Ok(BalanceSheet {
        as_of: as_of.to_owned(),
        total_assets: total(currency, &assets)?,
        total_liabilities: total(currency, &liabilities)?,
        total_equity: total(currency, &equity)?.checked_add(unclosed_profit)?,
        assets,
        liabilities,
        equity,
        unclosed_profit,
        snapshot,
    })
}

// ------------------------------------------------- cash basis (daňová evidence)

/// One cash-basis recognition: income or expense recognised when money moved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CashBasisLine {
    /// The payment's date.
    pub date: String,
    /// The payment (or offset) entry.
    pub cash_entry_id: i64,
    /// The entry it settled (an invoice), or `None` for a direct cash expense or income.
    pub settled_entry_id: Option<i64>,
    /// The revenue or expense account.
    pub account: String,
    /// Income or expense.
    pub direction: Direction,
    /// Income-tax treatment.
    pub tax_treatment: TaxTreatment,
    /// Positive for income received or expense paid; negative when reversed.
    pub amount: Money,
}

/// A total per direction and tax treatment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CashBasisTotal {
    /// Income or expense.
    pub direction: Direction,
    /// Income-tax treatment.
    pub tax_treatment: TaxTreatment,
    /// The total.
    pub amount: Money,
}

/// Income and expense recognised at settlement (*daňová evidence*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CashBasis {
    /// First day.
    pub from: String,
    /// Last day, inclusive.
    pub to: String,
    /// Every recognition, by date.
    pub lines: Vec<CashBasisLine>,
    /// Totals per direction and treatment.
    pub totals: Vec<CashBasisTotal>,
    /// Income with treatment `taxable`.
    pub taxable_income: Money,
    /// Expenses with treatment `deductible`.
    pub deductible_expenses: Money,
    /// What it read (everything up to `to`: partial payments depend on earlier ones).
    pub snapshot: Snapshot,
}

/// `value × part / whole`, rounded half to even, in integers.
fn prorate(value: i64, part: i64, whole: i64) -> Result<i64, LedgerError> {
    if whole == 0 {
        return Ok(0);
    }
    let numerator = i128::from(value) * i128::from(part);
    let whole = i128::from(whole);
    let (q, r) = (numerator.div_euclid(whole), numerator.rem_euclid(whole));
    let twice = 2 * r;
    let rounded = if twice > whole || (twice == whole && q % 2 != 0) {
        q + 1
    } else {
        q
    };
    i64::try_from(rounded).map_err(|_| skyla_money::MoneyError::Overflow.into())
}

/// The treatment of a line with none of its own: the account's category
/// default when all its categories agree, else taxable income or deductible expense.
fn default_treatments(conn: &Connection) -> Result<HashMap<i64, TaxTreatment>, LedgerError> {
    let mut stmt = conn.prepare_cached(
        "SELECT account_id, min(tax_treatment) FROM category GROUP BY account_id
         HAVING count(DISTINCT tax_treatment) = 1",
    )?;
    let map = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, treatment_from(1, &r.get::<_, String>(1)?)?))
        })?
        .collect::<Result<_, _>>()?;
    Ok(map)
}

struct ResultLine {
    account_id: i64,
    code: String,
    kind: AccountKind,
    amount: i64,
    treatment: Option<TaxTreatment>,
}

fn result_lines(conn: &Connection, entry_id: i64) -> Result<Vec<ResultLine>, LedgerError> {
    let mut stmt = conn.prepare_cached(
        "SELECT p.account_id, a.code, a.kind, p.amount_func_minor, p.tax_treatment
         FROM posting p JOIN account a ON a.id = p.account_id
         WHERE p.entry_id = ?1 AND a.kind IN ('revenue', 'expense') ORDER BY p.line_no",
    )?;
    let lines = stmt
        .query_map([entry_id], |r| {
            Ok(ResultLine {
                account_id: r.get(0)?,
                code: r.get(1)?,
                kind: kind_of(&r.get::<_, String>(2)?),
                amount: r.get(3)?,
                treatment: r
                    .get::<_, Option<String>>(4)?
                    .map(|t| treatment_from(4, &t))
                    .transpose()?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(lines)
}

/// Cash basis for payments dated `from..=to`. A payment entry recognises its
/// own revenue and expense lines in full, and a pro-rata share of the revenue
/// and expense lines of every entry it settles (VAT and balance-sheet lines
/// are never income). Shares are cumulative, so an invoice paid in parts is
/// recognised exactly once in total.
pub fn cash_basis(conn: &Connection, from: &str, to: &str) -> Result<CashBasis, LedgerError> {
    check_range(Some(from), to)?;
    let currency = functional_currency(conn)?;
    let defaults = default_treatments(conn)?;
    let mut lines = Vec::new();
    let mut push = |date: &str, cash: i64, settled: Option<i64>, l: &ResultLine, amount: i64| {
        if amount == 0 {
            return;
        }
        let (direction, natural) = if l.kind == AccountKind::Revenue {
            (Direction::Income, -amount)
        } else {
            (Direction::Expense, amount)
        };
        let tax_treatment = l
            .treatment
            .or_else(|| defaults.get(&l.account_id).copied())
            .unwrap_or(if direction == Direction::Income {
                TaxTreatment::Taxable
            } else {
                TaxTreatment::Deductible
            });
        lines.push(CashBasisLine {
            date: date.to_owned(),
            cash_entry_id: cash,
            settled_entry_id: settled,
            account: l.code.clone(),
            direction,
            tax_treatment,
            amount: Money::new(natural, currency),
        });
    };

    // Direct recognition: revenue and expense lines of entries that move cash
    // or settle something, dated in the range.
    let events: Vec<(i64, String)> = conn
        .prepare_cached(
            "SELECT e.id, e.entry_date FROM journal_entry e
             WHERE e.status = 'posted' AND e.entry_date BETWEEN ?1 AND ?2
               AND (EXISTS (SELECT 1 FROM settlement s WHERE s.cash_entry_id = e.id)
                    OR EXISTS (SELECT 1 FROM posting p JOIN account a ON a.id = p.account_id
                               WHERE p.entry_id = e.id AND a.cash = 1))
             ORDER BY e.entry_date, e.posted_seq",
        )?
        .query_map([from, to], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (id, date) in &events {
        for l in result_lines(conn, *id)? {
            push(date, *id, None, &l, l.amount);
        }
    }

    // Settled entries: cumulative pro-rata over every payment up to `to`.
    let links: Vec<(i64, i64, String, i64)> = conn
        .prepare_cached(
            "SELECT s.settled_entry_id, s.cash_entry_id, c.entry_date, s.amount_func_minor
             FROM settlement s JOIN journal_entry c ON c.id = s.cash_entry_id
             WHERE c.status = 'posted' AND c.entry_date <= ?1
             ORDER BY s.settled_entry_id, c.entry_date, c.posted_seq",
        )?
        .query_map([to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    let mut i = 0;
    while i < links.len() {
        let settled = links[i].0;
        let gross: i64 = conn.query_row(
            "SELECT coalesce(sum(amount_func_minor), 0) FROM posting WHERE entry_id = ?1 AND amount_func_minor > 0",
            [settled],
            |r| r.get(0),
        )?;
        let settled_lines = result_lines(conn, settled)?;
        let mut cumulative = 0_i64;
        while i < links.len() && links[i].0 == settled {
            let (_, cash, ref date, amount) = links[i];
            let before = cumulative;
            cumulative += amount;
            if date.as_str() >= from {
                for l in &settled_lines {
                    let share =
                        prorate(l.amount, cumulative, gross)? - prorate(l.amount, before, gross)?;
                    push(date, cash, Some(settled), l, share);
                }
            }
            i += 1;
        }
    }
    lines.sort_by(|a, b| (&a.date, a.cash_entry_id).cmp(&(&b.date, b.cash_entry_id)));

    let order = [
        (Direction::Income, TaxTreatment::Taxable),
        (Direction::Income, TaxTreatment::Exempt),
        (Direction::Income, TaxTreatment::NotTaxRelevant),
        (Direction::Expense, TaxTreatment::Deductible),
        (Direction::Expense, TaxTreatment::NonDeductible),
        (Direction::Expense, TaxTreatment::NotTaxRelevant),
    ];
    let mut totals = Vec::new();
    for (direction, tax_treatment) in order {
        let matching = lines
            .iter()
            .filter(|l| l.direction == direction && l.tax_treatment == tax_treatment);
        if matching.clone().next().is_some() {
            totals.push(CashBasisTotal {
                direction,
                tax_treatment,
                amount: Money::sum(currency, matching.map(|l| l.amount))?,
            });
        }
    }
    let pick = |d, t| {
        totals
            .iter()
            .find(|x: &&CashBasisTotal| x.direction == d && x.tax_treatment == t)
            .map_or(Money::new(0, currency), |x| x.amount)
    };
    Ok(CashBasis {
        from: from.to_owned(),
        to: to.to_owned(),
        taxable_income: pick(Direction::Income, TaxTreatment::Taxable),
        deductible_expenses: pick(Direction::Expense, TaxTreatment::Deductible),
        lines,
        totals,
        snapshot: snapshot(conn, "cash_basis", None, to, from)?,
    })
}

// ------------------------------------------------------------- VAT ledger

/// Which part of a VAT code's postings a form row takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VatPart {
    /// Lines with the code on non-tax accounts (the tax base).
    Base,
    /// All lines with the code on tax accounts.
    Tax,
    /// Debit tax lines only (input VAT, e.g. the deductible half of a reverse charge).
    TaxDebit,
    /// Credit tax lines only (output VAT, e.g. the payable half of a reverse charge).
    TaxCredit,
}

/// One mapping from a VAT code to a form row. Comes from the rule pack
/// (`form-row mappings`, `DESIGN.md` §3.4); the kernel only applies it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct VatRowRule {
    /// The posting's `vat_code`.
    pub vat_code: String,
    /// Form row, e.g. `"1"` or `"40"`.
    pub row: String,
    /// Which part feeds the row.
    pub part: VatPart,
    /// True to show credits as positive (output VAT and sales bases).
    pub credit_positive: bool,
}

/// Raw totals for one VAT code (debit positive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VatCodeTotals {
    /// The code.
    pub vat_code: String,
    /// Net of base lines.
    pub base: Money,
    /// Sum of debit tax lines.
    pub tax_debit: Money,
    /// Sum of credit tax lines (negative or zero).
    pub tax_credit: Money,
}

/// One form row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VatFormRow {
    /// Row number as printed on the form.
    pub row: String,
    /// Tax base.
    pub base: Money,
    /// Tax.
    pub tax: Money,
}

/// The VAT ledger for a date range: per code and per form row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VatLedger {
    /// First day.
    pub from: String,
    /// Last day, inclusive.
    pub to: String,
    /// Per VAT code, by code.
    pub codes: Vec<VatCodeTotals>,
    /// Per form row, in the order the rules first name them.
    pub rows: Vec<VatFormRow>,
    /// Codes with postings but no rule. A return can't be filed while any exist.
    pub unmapped: Vec<String>,
    /// What it read.
    pub snapshot: Snapshot,
}

/// The VAT ledger for entries dated `from..=to`. `tax_accounts` are the VAT
/// account codes (CZ: `343`); their analytic sub-accounts count too.
pub fn vat_ledger(
    conn: &Connection,
    from: &str,
    to: &str,
    tax_accounts: &[&str],
    rules: &[VatRowRule],
) -> Result<VatLedger, LedgerError> {
    check_range(Some(from), to)?;
    let currency = functional_currency(conn)?;
    let is_tax = |code: &str| {
        tax_accounts.iter().any(|t| {
            code == *t
                || code
                    .strip_prefix(t)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
    };
    let mut stmt = conn.prepare_cached(
        "SELECT p.vat_code, a.code, p.amount_func_minor FROM posting p
         JOIN journal_entry e ON e.id = p.entry_id JOIN account a ON a.id = p.account_id
         WHERE e.status = 'posted' AND e.entry_date BETWEEN ?1 AND ?2 AND p.vat_code IS NOT NULL",
    )?;
    let mut by_code: std::collections::BTreeMap<String, (i64, i64, i64)> = Default::default();
    let mut rows_iter = stmt.query([from, to])?;
    while let Some(r) = rows_iter.next()? {
        let (code, account, amount): (String, String, i64) = (r.get(0)?, r.get(1)?, r.get(2)?);
        let t = by_code.entry(code).or_default();
        match (is_tax(&account), amount > 0) {
            (false, _) => t.0 += amount,
            (true, true) => t.1 += amount,
            (true, false) => t.2 += amount,
        }
    }
    let codes: Vec<VatCodeTotals> = by_code
        .iter()
        .map(|(code, (base, debit, credit))| VatCodeTotals {
            vat_code: code.clone(),
            base: Money::new(*base, currency),
            tax_debit: Money::new(*debit, currency),
            tax_credit: Money::new(*credit, currency),
        })
        .collect();
    let mut rows: Vec<(String, i64, i64)> = Vec::new();
    for rule in rules {
        let (base, debit, credit) = by_code.get(&rule.vat_code).copied().unwrap_or_default();
        let raw = match rule.part {
            VatPart::Base => base,
            VatPart::Tax => debit + credit,
            VatPart::TaxDebit => debit,
            VatPart::TaxCredit => credit,
        };
        let value = if rule.credit_positive { -raw } else { raw };
        let index = match rows.iter().position(|r| r.0 == rule.row) {
            Some(i) => i,
            None => {
                rows.push((rule.row.clone(), 0, 0));
                rows.len() - 1
            }
        };
        if rule.part == VatPart::Base {
            rows[index].1 += value;
        } else {
            rows[index].2 += value;
        }
    }
    let unmapped = by_code
        .keys()
        .filter(|code| !rules.iter().any(|r| &r.vat_code == *code))
        .cloned()
        .collect();
    let parameters = format!(
        "{tax_accounts:?}|{}",
        rules
            .iter()
            .map(|r| format!(
                "{}:{}:{:?}:{}",
                r.vat_code, r.row, r.part, r.credit_positive
            ))
            .collect::<Vec<_>>()
            .join(",")
    );
    Ok(VatLedger {
        from: from.to_owned(),
        to: to.to_owned(),
        codes,
        rows: rows
            .into_iter()
            .map(|(row, base, tax)| VatFormRow {
                row,
                base: Money::new(base, currency),
                tax: Money::new(tax, currency),
            })
            .collect(),
        unmapped,
        snapshot: snapshot(conn, "vat_ledger", Some(from), to, &parameters)?,
    })
}
