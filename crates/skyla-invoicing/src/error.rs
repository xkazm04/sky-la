/// Errors from invoicing.
#[derive(Debug, thiserror::Error)]
pub enum InvoicingError {
    /// The ledger refused the posting (its message names the rule).
    #[error(transparent)]
    Ledger(#[from] skyla_ledger::LedgerError),
    /// The rule pack lacks a value the document needs.
    #[error(transparent)]
    Rules(#[from] skyla_rules::RulesError),
    /// Money arithmetic failed.
    #[error(transparent)]
    Money(#[from] skyla_money::MoneyError),
    /// No document with this id.
    #[error("no document {0}")]
    NotFound(i64),
    /// No series with this code.
    #[error("no number series {0}")]
    UnknownSeries(String),
    /// Issued documents never change; correct them with a credit note.
    #[error("document {0} is issued and can't change; issue a credit note instead")]
    Issued(i64),
    /// The document or request is invalid; every problem is listed.
    #[error("{}", .0.join("; "))]
    Invalid(Vec<String>),
    /// A database rule rejected the change.
    #[error("invoicing rule violated: {0}")]
    Rule(String),
    /// SQLite error.
    #[error(transparent)]
    Sql(rusqlite::Error),
}

impl From<rusqlite::Error> for InvoicingError {
    fn from(error: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(_, Some(message)) = &error
            && let Some(rule) = message.strip_prefix("invoicing: ")
        {
            return Self::Rule(rule.to_owned());
        }
        Self::Sql(error)
    }
}
