use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::LedgerError;

/// What an account represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    /// Things owned (balance sheet, left side).
    Asset,
    /// Obligations (balance sheet, right side).
    Liability,
    /// Owner's capital and results.
    Equity,
    /// Income (P&L).
    Revenue,
    /// Costs (P&L).
    Expense,
    /// Opening, closing and P&L summary accounts.
    Closing,
}

/// The side on which an account's balance normally grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalSide {
    /// Debit-normal (MD / Má dáti).
    Debit,
    /// Credit-normal (D / Dal).
    Credit,
}

/// Which side of the business a category belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Money earned.
    Income,
    /// Money spent.
    Expense,
    /// The owner's own money moving in or out.
    Owner,
}

/// How an amount counts for income tax, by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxTreatment {
    /// Income included in the tax base.
    Taxable,
    /// Income exempt from tax.
    Exempt,
    /// Expense that reduces the tax base.
    Deductible,
    /// Expense kept out of the tax base.
    NonDeductible,
    /// Neither income nor expense (e.g. owner's withdrawals).
    NotTaxRelevant,
}

impl AccountKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Asset => "asset",
            Self::Liability => "liability",
            Self::Equity => "equity",
            Self::Revenue => "revenue",
            Self::Expense => "expense",
            Self::Closing => "closing",
        }
    }
}

impl NormalSide {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Debit => "debit",
            Self::Credit => "credit",
        }
    }
}

impl Direction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Income => "income",
            Self::Expense => "expense",
            Self::Owner => "owner",
        }
    }
}

impl TaxTreatment {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Taxable => "taxable",
            Self::Exempt => "exempt",
            Self::Deductible => "deductible",
            Self::NonDeductible => "non_deductible",
            Self::NotTaxRelevant => "not_tax_relevant",
        }
    }
}

/// One account in a chart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountSpec {
    /// `221` for a synthetic account, `221.001` for an analytic one under it.
    pub code: String,
    /// Czech name.
    pub name_cs: String,
    /// English name.
    pub name_en: String,
    /// What the account represents.
    pub kind: AccountKind,
    /// Debit- or credit-normal.
    pub normal_side: NormalSide,
    /// A contra account runs against its kind's usual side (accumulated depreciation).
    #[serde(default)]
    pub contra: bool,
}

/// A user-facing category mapped onto an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategorySpec {
    /// Stable identifier, e.g. `software-subscriptions`.
    pub key: String,
    /// Czech name.
    pub name_cs: String,
    /// English name.
    pub name_en: String,
    /// Income, expense or owner.
    pub direction: Direction,
    /// Code of the leaf account postings go to.
    pub account: String,
    /// Default tax treatment for new postings.
    pub tax_treatment: TaxTreatment,
}

/// A chart of accounts with categories, as shipped in a rule pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartSpec {
    /// Pack identifier, e.g. `cz-chart`.
    pub id: String,
    /// Pack version.
    pub version: String,
    /// The legal source the chart follows.
    pub citation: String,
    /// Accounts.
    #[serde(rename = "account")]
    pub accounts: Vec<AccountSpec>,
    /// Categories.
    #[serde(default, rename = "category")]
    pub categories: Vec<CategorySpec>,
}

/// `221` → None, `221.001` → `221`, `221.001.01` → `221.001`.
pub(crate) fn parent_code(code: &str) -> Option<&str> {
    code.rsplit_once('.').map(|(parent, _)| parent)
}

fn is_valid_code(code: &str) -> bool {
    let mut segments = code.split('.');
    let synthetic = segments.next().unwrap_or("");
    synthetic.len() == 3
        && synthetic.bytes().all(|b| b.is_ascii_digit())
        && segments
            .all(|s| !s.is_empty() && s.len() <= 6 && s.bytes().all(|b| b.is_ascii_alphanumeric()))
}

impl ChartSpec {
    /// Parses TOML and validates the result.
    pub fn from_toml(text: &str) -> Result<Self, LedgerError> {
        let spec: Self =
            toml::from_str(text).map_err(|e| LedgerError::ChartFormat(e.to_string()))?;
        spec.validate()?;
        Ok(spec)
    }

    /// Checks the chart is internally consistent; reports every problem at once.
    pub fn validate(&self) -> Result<(), LedgerError> {
        let mut problems = Vec::new();
        let mut by_code: HashMap<&str, &AccountSpec> = HashMap::new();
        for account in &self.accounts {
            if !is_valid_code(&account.code) {
                problems.push(format!(
                    "account {:?}: code must be three digits, optionally followed by .segments",
                    account.code
                ));
            }
            if by_code.insert(&account.code, account).is_some() {
                problems.push(format!("account {}: duplicate code", account.code));
            }
            if account.name_cs.trim().is_empty() || account.name_en.trim().is_empty() {
                problems.push(format!("account {}: both names are required", account.code));
            }
            let expected = match account.kind {
                AccountKind::Asset | AccountKind::Expense => Some(NormalSide::Debit),
                AccountKind::Liability | AccountKind::Revenue => Some(NormalSide::Credit),
                AccountKind::Equity | AccountKind::Closing => None,
            };
            match (expected, account.contra) {
                (Some(side), false) if side != account.normal_side => problems.push(format!(
                    "account {}: a {} account is {}-normal unless marked contra",
                    account.code,
                    account.kind.as_str(),
                    side.as_str()
                )),
                (Some(side), true) if side == account.normal_side => {
                    problems.push(format!(
                        "account {}: marked contra but uses the usual side",
                        account.code
                    ));
                }
                (None, true) => problems.push(format!(
                    "account {}: only asset, liability, revenue and expense accounts can be contra",
                    account.code
                )),
                _ => {}
            }
        }
        let parents: HashSet<&str> = self
            .accounts
            .iter()
            .filter_map(|a| parent_code(&a.code))
            .collect();
        for account in &self.accounts {
            if let Some(parent) = parent_code(&account.code) {
                match by_code.get(parent) {
                    None => problems.push(format!(
                        "account {}: parent {parent} is missing",
                        account.code
                    )),
                    Some(p) if p.kind != account.kind || p.normal_side != account.normal_side => {
                        problems.push(format!(
                            "account {}: kind and side must match parent {parent}",
                            account.code
                        ));
                    }
                    Some(_) => {}
                }
            }
        }

        let mut keys = HashSet::new();
        for category in &self.categories {
            if !keys.insert(category.key.as_str()) {
                problems.push(format!("category {}: duplicate key", category.key));
            }
            let Some(account) = by_code.get(category.account.as_str()) else {
                problems.push(format!(
                    "category {}: account {} does not exist",
                    category.key, category.account
                ));
                continue;
            };
            if parents.contains(category.account.as_str()) {
                problems.push(format!(
                    "category {}: account {} has sub-accounts, so it can't be posted to",
                    category.key, category.account
                ));
            }
            let kind_ok = matches!(
                (category.direction, account.kind),
                (Direction::Income, AccountKind::Revenue)
                    | (Direction::Expense, AccountKind::Expense)
                    | (Direction::Owner, AccountKind::Equity)
            );
            if !kind_ok {
                problems.push(format!(
                    "category {}: a {} category can't map to {} account {}",
                    category.key,
                    category.direction.as_str(),
                    account.kind.as_str(),
                    account.code
                ));
            }
            let treatment_ok = matches!(
                (category.direction, category.tax_treatment),
                (
                    Direction::Income,
                    TaxTreatment::Taxable | TaxTreatment::Exempt
                ) | (
                    Direction::Expense,
                    TaxTreatment::Deductible | TaxTreatment::NonDeductible
                ) | (Direction::Owner, TaxTreatment::NotTaxRelevant)
            );
            if !treatment_ok {
                problems.push(format!(
                    "category {}: tax treatment {} doesn't fit a {} category",
                    category.key,
                    category.tax_treatment.as_str(),
                    category.direction.as_str()
                ));
            }
        }

        if problems.is_empty() {
            Ok(())
        } else {
            Err(LedgerError::InvalidChart(problems))
        }
    }
}
