use serde::{Deserialize, Serialize};
use specta::Type;

/// Errors from the application core.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// The ledger refused or failed.
    #[error(transparent)]
    Ledger(#[from] skyla_ledger::LedgerError),
    /// Money arithmetic failed.
    #[error(transparent)]
    Money(#[from] skyla_money::MoneyError),
    /// The demo data is inconsistent with the ledger.
    #[error("demo data: {0}")]
    Demo(String),
    /// A request was malformed.
    #[error("{0}")]
    BadRequest(String),
    /// The rule pack refused or lacks a value.
    #[error(transparent)]
    Rules(#[from] skyla_rules::RulesError),
    /// An amount doesn't fit a JavaScript number exactly.
    #[error("amount {0} is beyond the range the webview can show exactly")]
    OutOfRange(i64),
}

/// The error every command returns to the webview: a stable code and a
/// message for people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IpcFailure {
    /// `ledger`, `bad_request`, `demo`, `out_of_range`, `money`.
    pub code: String,
    /// What went wrong.
    pub message: String,
}

impl From<CoreError> for IpcFailure {
    fn from(error: CoreError) -> Self {
        let code = match &error {
            CoreError::Ledger(_) => "ledger",
            CoreError::Money(_) => "money",
            CoreError::Rules(_) => "rules",
            CoreError::Demo(_) => "demo",
            CoreError::BadRequest(_) => "bad_request",
            CoreError::OutOfRange(_) => "out_of_range",
        };
        Self {
            code: code.to_owned(),
            message: error.to_string(),
        }
    }
}
