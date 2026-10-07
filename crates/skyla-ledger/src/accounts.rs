use rusqlite::types::FromSqlError;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;

use crate::chart::parent_code;
use crate::{AccountKind, ChartSpec, Direction, LedgerError, NormalSide, TaxTreatment};

/// An account as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Account {
    /// Account code.
    pub code: String,
    /// Czech name.
    pub name_cs: String,
    /// English name.
    pub name_en: String,
    /// What it represents.
    pub kind: AccountKind,
    /// Debit- or credit-normal.
    pub normal_side: NormalSide,
    /// Runs against its kind's usual side.
    pub contra: bool,
    /// Parent code for analytic accounts.
    pub parent_code: Option<String>,
    /// True when it has no sub-accounts, so it can be posted to.
    pub is_leaf: bool,
    /// Inactive accounts keep their history but take no new postings.
    pub active: bool,
}

/// A category as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Category {
    /// Stable key.
    pub key: String,
    /// Czech name.
    pub name_cs: String,
    /// English name.
    pub name_en: String,
    /// Income, expense or owner.
    pub direction: Direction,
    /// The account postings go to.
    pub account_code: String,
    /// Default tax treatment.
    pub tax_treatment: TaxTreatment,
}

fn bad_value(column: usize, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        Box::new(FromSqlError::Other(
            format!("unexpected value {value:?}").into(),
        )),
    )
}

fn kind_from(column: usize, s: &str) -> rusqlite::Result<AccountKind> {
    Ok(match s {
        "asset" => AccountKind::Asset,
        "liability" => AccountKind::Liability,
        "equity" => AccountKind::Equity,
        "revenue" => AccountKind::Revenue,
        "expense" => AccountKind::Expense,
        "closing" => AccountKind::Closing,
        other => return Err(bad_value(column, other)),
    })
}

fn side_from(column: usize, s: &str) -> rusqlite::Result<NormalSide> {
    match s {
        "debit" => Ok(NormalSide::Debit),
        "credit" => Ok(NormalSide::Credit),
        other => Err(bad_value(column, other)),
    }
}

fn direction_from(column: usize, s: &str) -> rusqlite::Result<Direction> {
    match s {
        "income" => Ok(Direction::Income),
        "expense" => Ok(Direction::Expense),
        "owner" => Ok(Direction::Owner),
        other => Err(bad_value(column, other)),
    }
}

fn treatment_from(column: usize, s: &str) -> rusqlite::Result<TaxTreatment> {
    Ok(match s {
        "taxable" => TaxTreatment::Taxable,
        "exempt" => TaxTreatment::Exempt,
        "deductible" => TaxTreatment::Deductible,
        "non_deductible" => TaxTreatment::NonDeductible,
        "not_tax_relevant" => TaxTreatment::NotTaxRelevant,
        other => return Err(bad_value(column, other)),
    })
}

const ACCOUNT_COLUMNS: &str =
    "a.code, a.name_cs, a.name_en, a.kind, a.normal_side, a.contra, p.code, a.is_leaf, a.active";

fn account_from_row(row: &Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        code: row.get(0)?,
        name_cs: row.get(1)?,
        name_en: row.get(2)?,
        kind: kind_from(3, &row.get::<_, String>(3)?)?,
        normal_side: side_from(4, &row.get::<_, String>(4)?)?,
        contra: row.get(5)?,
        parent_code: row.get(6)?,
        is_leaf: row.get(7)?,
        active: row.get(8)?,
    })
}

fn account_id(conn: &Connection, code: &str) -> Result<i64, LedgerError> {
    conn.query_row("SELECT id FROM account WHERE code = ?1", [code], |row| {
        row.get(0)
    })
    .optional()?
    .ok_or_else(|| LedgerError::UnknownAccount(code.to_owned()))
}

/// Validates `spec` and inserts its accounts and categories in one transaction.
pub fn seed_chart(conn: &Connection, spec: &ChartSpec) -> Result<(), LedgerError> {
    spec.validate()?;
    let tx = conn.unchecked_transaction()?;
    let mut accounts: Vec<_> = spec.accounts.iter().collect();
    // Parents before children: fewer segments first.
    accounts.sort_by_key(|a| (a.code.matches('.').count(), a.code.clone()));
    for account in accounts {
        let parent_id = match parent_code(&account.code) {
            Some(parent) => Some(account_id(&tx, parent)?),
            None => None,
        };
        tx.execute(
            "INSERT INTO account (code, name_cs, name_en, kind, normal_side, contra, parent_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![account.code, account.name_cs, account.name_en, account.kind.as_str(), account.normal_side.as_str(), account.contra, parent_id],
        )?;
    }
    for category in &spec.categories {
        let account = account_id(&tx, &category.account)?;
        tx.execute(
            "INSERT INTO category (key, name_cs, name_en, direction, account_id, tax_treatment) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![category.key, category.name_cs, category.name_en, category.direction.as_str(), account, category.tax_treatment.as_str()],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Every account, ordered by code.
pub fn list_accounts(conn: &Connection) -> Result<Vec<Account>, LedgerError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ACCOUNT_COLUMNS} FROM account_with_leaf a LEFT JOIN account p ON p.id = a.parent_id ORDER BY a.code"
    ))?;
    let accounts = stmt
        .query_map([], account_from_row)?
        .collect::<Result<_, _>>()?;
    Ok(accounts)
}

/// Every category, ordered by key.
pub fn list_categories(conn: &Connection) -> Result<Vec<Category>, LedgerError> {
    let mut stmt = conn.prepare(
        "SELECT c.key, c.name_cs, c.name_en, c.direction, a.code, c.tax_treatment
         FROM category c JOIN account a ON a.id = c.account_id ORDER BY c.key",
    )?;
    let categories = stmt
        .query_map([], |row| {
            Ok(Category {
                key: row.get(0)?,
                name_cs: row.get(1)?,
                name_en: row.get(2)?,
                direction: direction_from(3, &row.get::<_, String>(3)?)?,
                account_code: row.get(4)?,
                tax_treatment: treatment_from(5, &row.get::<_, String>(5)?)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(categories)
}

/// Adds an analytic account `parent.suffix` (e.g. `221.001` for one bank
/// account), inheriting the parent's kind and side. The parent stops being
/// postable.
pub fn add_analytic_account(
    conn: &Connection,
    parent: &str,
    suffix: &str,
    name_cs: &str,
    name_en: &str,
) -> Result<Account, LedgerError> {
    let parent_id = account_id(conn, parent)?;
    let code = format!("{parent}.{suffix}");
    conn.execute(
        "INSERT INTO account (code, name_cs, name_en, kind, normal_side, contra, parent_id)
         SELECT ?1, ?2, ?3, kind, normal_side, contra, id FROM account WHERE id = ?4",
        params![code, name_cs, name_en, parent_id],
    )?;
    let account = conn.query_row(
        &format!("SELECT {ACCOUNT_COLUMNS} FROM account_with_leaf a LEFT JOIN account p ON p.id = a.parent_id WHERE a.code = ?1"),
        [&code],
        account_from_row,
    )?;
    Ok(account)
}

/// Opens an accounting period. Dates are ISO `YYYY-MM-DD`; periods can't overlap.
pub fn open_period(conn: &Connection, starts_on: &str, ends_on: &str) -> Result<i64, LedgerError> {
    conn.execute(
        "INSERT INTO period (starts_on, ends_on) VALUES (?1, ?2)",
        [starts_on, ends_on],
    )?;
    Ok(conn.last_insert_rowid())
}
