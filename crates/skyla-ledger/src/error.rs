/// Errors from the ledger kernel.
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    /// The chart-of-accounts data is inconsistent. Every problem is listed.
    #[error("invalid chart of accounts:\n  {}", .0.join("\n  "))]
    InvalidChart(Vec<String>),
    /// The chart TOML can't be parsed.
    #[error("chart of accounts is not valid TOML: {0}")]
    ChartFormat(String),
    /// No account has this code.
    #[error("unknown account {0}")]
    UnknownAccount(String),
    /// A rule enforced by the database rejected the change (the message names it).
    #[error("ledger rule violated: {0}")]
    Rule(String),
    /// SQLite error.
    #[error(transparent)]
    Sql(rusqlite::Error),
}

impl From<rusqlite::Error> for LedgerError {
    fn from(error: rusqlite::Error) -> Self {
        // Triggers raise ABORT with a message starting "ledger:"; surface those as rule violations.
        if let rusqlite::Error::SqliteFailure(_, Some(message)) = &error
            && let Some(rule) = message.strip_prefix("ledger: ")
        {
            return Self::Rule(rule.to_owned());
        }
        Self::Sql(error)
    }
}
