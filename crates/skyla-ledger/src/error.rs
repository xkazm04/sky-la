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
    /// The entry's lines don't sum to zero in the functional currency.
    #[error("entry is unbalanced by {difference}")]
    Unbalanced {
        /// Sum of all lines; zero when balanced.
        difference: skyla_money::Money,
    },
    /// An entry needs at least two lines to post.
    #[error("an entry needs at least two lines to post")]
    TooFewLines,
    /// No period covers the entry date.
    #[error("no period covers {0}; open one first")]
    NoPeriod(String),
    /// The period covering the entry date is closed.
    #[error("the period covering {0} is closed")]
    PeriodClosed(String),
    /// Rule- and advisor-sourced entries need a human approver (invariant I7).
    #[error("this entry was proposed by a rule or the advisor and needs a human approver")]
    NeedsApproval,
    /// The account can't take postings.
    #[error("account {code} can't take postings: {reason}")]
    AccountNotPostable {
        /// The account code.
        code: String,
        /// Why not.
        reason: &'static str,
    },
    /// A line is malformed.
    #[error("line {line}: {reason}")]
    InvalidLine {
        /// One-based line number.
        line: usize,
        /// What's wrong.
        reason: String,
    },
    /// The entry is already posted, so it can't change.
    #[error("entry {0} is already posted")]
    AlreadyPosted(i64),
    /// No entry with this id.
    #[error("no journal entry {0}")]
    EntryNotFound(i64),
    /// Not an ISO `YYYY-MM-DD` calendar date.
    #[error("invalid date {0:?}; expected YYYY-MM-DD")]
    InvalidDate(String),
    /// The functional currency hasn't been set for this ledger.
    #[error("the functional currency isn't set")]
    FunctionalCurrencyNotSet,
    /// Money arithmetic failed (overflow or mixed currencies).
    #[error(transparent)]
    Money(#[from] skyla_money::MoneyError),
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
