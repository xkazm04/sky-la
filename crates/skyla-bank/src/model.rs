//! What every parser produces.

use serde::{Deserialize, Serialize};
use skyla_money::Money;

/// The statement formats sky-la reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// ISO 20022 `camt.053` (BankToCustomerStatement), versions 001.02 to 001.08.
    Camt053,
    /// SWIFT MT940.
    Mt940,
    /// ABO / GPC, the Czech banks' fixed-width export (`074` and `075` records).
    Gpc,
    /// CSV, read with a column profile.
    Csv,
}

/// The account a statement is for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// IBAN, when the file names one.
    pub iban: Option<String>,
    /// The domestic number (`prefix-number` or `number`), when the file names one.
    pub number: Option<String>,
    /// The bank code (`0800`), when the file names one.
    pub bank_code: Option<String>,
}

/// One booked line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankLine {
    /// Position in the file, from 1.
    pub sequence: u32,
    /// Booking date, `YYYY-MM-DD`.
    pub booking_date: String,
    /// Value date, when given.
    pub value_date: Option<String>,
    /// Signed: positive money in, negative money out.
    pub amount: Money,
    /// A reversal (storno) of an earlier line.
    pub reversal: bool,
    /// The other party's name.
    pub counterparty_name: Option<String>,
    /// The other party's account (IBAN or `number/bank`).
    pub counterparty_account: Option<String>,
    /// Variable symbol.
    pub vs: Option<String>,
    /// Constant symbol.
    pub ks: Option<String>,
    /// Specific symbol.
    pub ss: Option<String>,
    /// The payment message.
    pub message: Option<String>,
    /// The bank's own reference for the line.
    pub bank_ref: Option<String>,
}

/// A parsed statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    /// Which format it was.
    pub format: Format,
    /// The account.
    pub account: Account,
    /// Statement number or id, when given.
    pub number: Option<String>,
    /// First day covered.
    pub from: Option<String>,
    /// Last day covered.
    pub to: Option<String>,
    /// Opening balance, when given.
    pub opening: Option<Money>,
    /// Closing balance, when given.
    pub closing: Option<Money>,
    /// The lines, in file order.
    pub lines: Vec<BankLine>,
    /// What was skipped or guessed, for the user to see.
    pub warnings: Vec<String>,
}

/// Why a file couldn't be read. Parsers never panic; every failure is one of these.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BankError {
    /// The file isn't in the format, or is damaged.
    #[error("{format:?}{}: {message}", .line.map(|l| format!(" line {l}")).unwrap_or_default())]
    Malformed {
        /// The format being read.
        format: Format,
        /// The 1-based line (or record), when it applies.
        line: Option<usize>,
        /// What's wrong.
        message: String,
    },
    /// The file is larger than sky-la reads.
    #[error("the file is {size} bytes; statements up to {limit} bytes are read")]
    TooLarge {
        /// Its size.
        size: usize,
        /// The limit.
        limit: usize,
    },
    /// No known format recognises the file.
    #[error("this file isn't a CAMT.053, MT940, ABO/GPC or CSV statement")]
    Unrecognised,
}

pub(crate) fn malformed(
    format: Format,
    line: Option<usize>,
    message: impl Into<String>,
) -> BankError {
    BankError::Malformed {
        format,
        line,
        message: message.into(),
    }
}
