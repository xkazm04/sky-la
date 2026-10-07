//! Everything the webview receives. Field names are camelCase in JSON and in
//! the generated TypeScript (`packages/ipc/src/bindings.ts`). Amounts are
//! [`MoneyDto`]s in integer minor units; the webview formats, never computes.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::CoreError;

/// Largest integer a JavaScript number holds exactly (2^53 − 1).
pub const MAX_SAFE_MINOR: i64 = 9_007_199_254_740_991;

/// An amount in integer minor units (`8470000` CZK is 84 700,00 Kč).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MoneyDto {
    /// Minor units; always within ±(2^53 − 1).
    pub minor: i64,
    /// ISO 4217 code.
    pub currency: String,
}

impl TryFrom<skyla_money::Money> for MoneyDto {
    type Error = CoreError;
    fn try_from(money: skyla_money::Money) -> Result<Self, CoreError> {
        if money.minor().abs() > MAX_SAFE_MINOR {
            return Err(CoreError::OutOfRange(money.minor()));
        }
        Ok(Self {
            minor: money.minor(),
            currency: money.currency().code().to_owned(),
        })
    }
}

/// Build information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// Product name.
    pub name: String,
    /// Core version.
    pub version: String,
}

/// The open entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EntityDto {
    /// Shown in the source list.
    pub display_name: String,
    /// `OSVČ` or `s.r.o.`.
    pub legal_form: String,
    /// `monthly`, `quarterly` or `non-payer`.
    pub vat_period: String,
    /// ISO 4217.
    pub functional_currency: String,
    /// The date the books are seen from (today, or the demo's fixed date).
    pub as_of: String,
    /// Bank account label, masked.
    pub bank_name: String,
}

/// An accounting period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PeriodDto {
    /// Row id.
    pub id: i64,
    /// E.g. `Q3 2026`.
    pub label: String,
    /// First day.
    pub starts_on: String,
    /// Last day.
    pub ends_on: String,
    /// `open`, `closing` or `closed`.
    pub state: String,
}

/// What a report read; see `skyla_ledger::Snapshot`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDto {
    /// Posted entries read.
    pub entries: u32,
    /// Highest posting sequence number read.
    pub last_posted_seq: Option<i64>,
    /// SHA-256 hex.
    pub hash: String,
}

/// A statement line by synthetic account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StatementLineDto {
    /// Account code.
    pub code: String,
    /// Czech name.
    pub name_cs: String,
    /// English name.
    pub name_en: String,
    /// Amount as the statement reads.
    pub amount: MoneyDto,
}

/// Profit and loss.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfitAndLossDto {
    /// First day.
    pub from: String,
    /// Last day.
    pub to: String,
    /// Revenue lines.
    pub revenue: Vec<StatementLineDto>,
    /// Expense lines.
    pub expenses: Vec<StatementLineDto>,
    /// Sum of revenue.
    pub total_revenue: MoneyDto,
    /// Sum of expenses.
    pub total_expenses: MoneyDto,
    /// Revenue minus expenses.
    pub profit: MoneyDto,
    /// What it read.
    pub snapshot: SnapshotDto,
}

/// Balance sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSheetDto {
    /// The date.
    pub as_of: String,
    /// Asset lines.
    pub assets: Vec<StatementLineDto>,
    /// Liability lines.
    pub liabilities: Vec<StatementLineDto>,
    /// Equity lines.
    pub equity: Vec<StatementLineDto>,
    /// Profit not yet closed into equity.
    pub unclosed_profit: MoneyDto,
    /// Sum of assets.
    pub total_assets: MoneyDto,
    /// Sum of liabilities.
    pub total_liabilities: MoneyDto,
    /// Equity including the unclosed profit.
    pub total_equity: MoneyDto,
    /// Assets equal liabilities plus equity.
    pub balances: bool,
    /// What it read.
    pub snapshot: SnapshotDto,
}

/// A trial-balance row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrialBalanceRowDto {
    /// Leaf account code.
    pub code: String,
    /// Czech name.
    pub name_cs: String,
    /// English name.
    pub name_en: String,
    /// `asset`, `liability`, `equity`, `revenue`, `expense`, `closing`.
    pub kind: String,
    /// Debits.
    pub debit: MoneyDto,
    /// Credits (positive).
    pub credit: MoneyDto,
    /// Debit minus credit.
    pub balance: MoneyDto,
}

/// Trial balance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrialBalanceDto {
    /// First day, or none for everything up to `to`.
    pub from: Option<String>,
    /// Last day.
    pub to: String,
    /// Rows by code.
    pub rows: Vec<TrialBalanceRowDto>,
    /// Total debits.
    pub total_debit: MoneyDto,
    /// Total credits.
    pub total_credit: MoneyDto,
    /// What it read.
    pub snapshot: SnapshotDto,
}

/// A cash-basis recognition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CashBasisLineDto {
    /// Payment date.
    pub date: String,
    /// The payment entry.
    pub cash_entry_id: i64,
    /// The settled entry, if any.
    pub settled_entry_id: Option<i64>,
    /// Revenue or expense account.
    pub account: String,
    /// `income` or `expense`.
    pub direction: String,
    /// Income-tax treatment.
    pub tax_treatment: String,
    /// The recognised amount.
    pub amount: MoneyDto,
}

/// A cash-basis total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CashBasisTotalDto {
    /// `income` or `expense`.
    pub direction: String,
    /// Income-tax treatment.
    pub tax_treatment: String,
    /// The total.
    pub amount: MoneyDto,
}

/// Cash basis (*daňová evidence*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CashBasisDto {
    /// First day.
    pub from: String,
    /// Last day.
    pub to: String,
    /// Every recognition.
    pub lines: Vec<CashBasisLineDto>,
    /// Totals per direction and treatment.
    pub totals: Vec<CashBasisTotalDto>,
    /// Taxable income.
    pub taxable_income: MoneyDto,
    /// Deductible expenses.
    pub deductible_expenses: MoneyDto,
    /// What it read.
    pub snapshot: SnapshotDto,
}

/// A journal line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct JournalLineDto {
    /// One-based.
    pub line_no: i64,
    /// Account code.
    pub account: String,
    /// Account name (English).
    pub account_name: String,
    /// Amount in the line's currency; debit positive.
    pub amount: MoneyDto,
    /// Functional-currency amount.
    pub functional: MoneyDto,
    /// FX rate for foreign lines.
    pub fx_rate: Option<String>,
    /// VAT code.
    pub vat_code: Option<String>,
    /// Free text.
    pub memo: String,
}

/// A posted journal entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntryDto {
    /// Row id.
    pub id: i64,
    /// Accounting date.
    pub date: String,
    /// Gapless posting number.
    pub posted_seq: Option<i64>,
    /// `manual`, `invoice`, `bank`, `rule`, `advisor`, `reversal`, `opening`.
    pub source_kind: String,
    /// Origin reference.
    pub source_ref: Option<String>,
    /// Description.
    pub memo: String,
    /// Approver, when recorded.
    pub approved_by: Option<String>,
    /// The entry it reverses.
    pub reverses_id: Option<i64>,
    /// The lines.
    pub lines: Vec<JournalLineDto>,
}

/// Journal integrity for the status line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityDto {
    /// Every posted entry fits the hash chain.
    pub chain_intact: bool,
    /// Posted entries verified.
    pub entries_checked: u32,
    /// The verified chain head.
    pub head: Option<String>,
    /// The first break, described, if any.
    pub first_break: Option<String>,
    /// Debits equal credits across the journal.
    pub balanced: bool,
}

/// One invoice line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLineDto {
    /// What was supplied.
    pub description: String,
    /// Quantity as written, e.g. `56`.
    pub quantity: String,
    /// Unit, e.g. `h`.
    pub unit: String,
    /// Price per unit, excluding VAT.
    pub unit_price: MoneyDto,
    /// The rule pack's VAT code.
    pub vat_code: String,
    /// Its rate in percent on the document date, from the rule pack.
    pub vat_rate_percent: String,
    /// Quantity × unit price.
    pub base: MoneyDto,
    /// VAT on the line.
    pub vat: MoneyDto,
}

/// An issued (or drafted) invoice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceDto {
    /// Document id.
    pub id: i64,
    /// Invoice number, once issued.
    pub number: Option<String>,
    /// Client name.
    pub client: String,
    /// `draft`, `scheduled`, `open`, `overdue`, `partPaid`, `paid`, `credited`.
    pub status: String,
    /// Issue date (posted invoices).
    pub issued_on: Option<String>,
    /// Due date.
    pub due_on: Option<String>,
    /// When a scheduled invoice will be issued.
    pub scheduled_for: Option<String>,
    /// When it was paid in full.
    pub paid_on: Option<String>,
    /// Days past due, when overdue and open.
    pub days_overdue: Option<i64>,
    /// Excluding VAT.
    pub base: MoneyDto,
    /// VAT.
    pub vat: MoneyDto,
    /// Total.
    pub gross: MoneyDto,
    /// Paid so far.
    pub paid: MoneyDto,
    /// Credited by credit notes.
    pub credited: MoneyDto,
    /// Still open.
    pub open: MoneyDto,
    /// The lines.
    pub lines: Vec<InvoiceLineDto>,
    /// The posted journal entry, if issued.
    pub entry_id: Option<i64>,
    /// The rule pack its totals were fixed with, once issued.
    pub pack: Option<String>,
}

/// One weighted reason in a match score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScoreContributionDto {
    /// Why.
    pub reason: String,
    /// Signed weight as written, e.g. `+.40`.
    pub weight: String,
}

/// A candidate document for a bank line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MatchCandidateDto {
    /// E.g. `Invoice 2026-102 · Studio Brno`.
    pub label: String,
    /// Key facts.
    pub detail: String,
    /// `0.96`.
    pub score: String,
    /// `certain`, `likely`, `needs_you`, `unlikely`.
    pub confidence: String,
    /// The score as a sum of named contributions.
    pub contributions: Vec<ScoreContributionDto>,
}

/// A foreign-currency amount behind a bank line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ForeignAmountDto {
    /// The original amount.
    pub amount: MoneyDto,
    /// The conversion rate.
    pub rate: String,
}

/// A bank statement line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BankLineDto {
    /// Stable id.
    pub id: String,
    /// Booking date.
    pub date: String,
    /// Who.
    pub counterparty: String,
    /// VS, card or other reference.
    pub reference: String,
    /// Signed amount in the account currency.
    pub amount: MoneyDto,
    /// The original foreign amount, for card payments abroad.
    pub foreign: Option<ForeignAmountDto>,
    /// `open` or `matched`.
    pub status: String,
    /// What it was matched to.
    pub matched_to: Option<String>,
    /// The proposal for this line.
    pub proposal_id: Option<String>,
    /// Ranked candidates.
    pub candidates: Vec<MatchCandidateDto>,
}

/// An imported statement with its tie-out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BankStatementDto {
    /// Bank account label.
    pub account_name: String,
    /// Imported file.
    pub file: String,
    /// `CAMT.053`, `MT940`, `ABO`, `CSV`.
    pub format: String,
    /// First booking day.
    pub from: String,
    /// Last booking day.
    pub to: String,
    /// The ledger's balance before the statement.
    pub opening: MoneyDto,
    /// Sum of incoming lines.
    pub credits: MoneyDto,
    /// Sum of outgoing lines (negative).
    pub debits: MoneyDto,
    /// Opening plus every line.
    pub closing: MoneyDto,
    /// The closing balance the bank reported.
    pub reported_closing: MoneyDto,
    /// `closing == reported_closing`.
    pub ties_out: bool,
    /// The lines, newest first.
    pub lines: Vec<BankLineDto>,
}

/// One line of a proposed entry, split into debit and credit for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProposedLineDto {
    /// Account code.
    pub account: String,
    /// Account name (English).
    pub account_name: String,
    /// Debit, if this is a debit line.
    pub debit: Option<MoneyDto>,
    /// Credit, if this is a credit line.
    pub credit: Option<MoneyDto>,
    /// VAT code.
    pub vat_code: Option<String>,
}

/// A journal entry proposed by a rule, the matcher or an advisor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProposedEntryDto {
    /// Accounting date.
    pub date: String,
    /// Description.
    pub memo: String,
    /// The lines.
    pub lines: Vec<ProposedLineDto>,
    /// Total debits.
    pub total_debit: MoneyDto,
    /// Total credits (positive).
    pub total_credit: MoneyDto,
    /// Debits equal credits; the kernel re-checks on approval.
    pub balanced: bool,
}

/// An inbox item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDto {
    /// Stable id.
    pub id: String,
    /// `posting`, `deadline`, `advice`, `reminder`.
    pub kind: String,
    /// One line.
    pub title: String,
    /// Second line.
    pub detail: String,
    /// `certain`, `likely`, `needs_you`.
    pub confidence: Option<String>,
    /// `rule`, `matcher`, `advisor`, `calendar`.
    pub source_kind: String,
    /// Shown as the origin, e.g. `Rule “EU SaaS”`.
    pub source: String,
    /// The bank line it came from.
    pub bank_line_id: Option<String>,
    /// For deadlines.
    pub due_on: Option<String>,
    /// The headline amount, signed as the bank line.
    pub amount: Option<MoneyDto>,
    /// For postings.
    pub entry: Option<ProposedEntryDto>,
    /// Why.
    pub reasons: Vec<String>,
}

/// One row of the DPH return.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatReturnRowDto {
    /// Row as printed on the form.
    pub row: String,
    /// What feeds it.
    pub label: String,
    /// `output` (tax due) or `input` (tax claimed).
    pub side: String,
    /// Tax base.
    pub base: MoneyDto,
    /// Tax.
    pub tax: MoneyDto,
}

/// A DPH return for one period, computed from the ledger with the rule pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatReturnDto {
    /// First day.
    pub from: String,
    /// Last day.
    pub to: String,
    /// The pack that mapped it, `cz-2026@2026.1`.
    pub pack: String,
    /// `draft` or `reviewed`.
    pub pack_review: String,
    /// Rows in form order.
    pub rows: Vec<VatReturnRowDto>,
    /// Tax due on supplies and reverse charges.
    pub output_tax: MoneyDto,
    /// Tax claimed.
    pub input_tax: MoneyDto,
    /// Output minus input: positive to pay, negative for an excess deduction.
    pub payable: MoneyDto,
    /// VAT codes posted but not mapped; the return isn't complete while any exist.
    pub unmapped: Vec<String>,
    /// Filing and payment deadline (shifted to a working day).
    pub due_on: String,
    /// What it read.
    pub snapshot: SnapshotDto,
}

/// One statutory value with its source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PackValueDto {
    /// Dotted key.
    pub key: String,
    /// `percent`, `amount`, `days`, `rounding`, `flag`.
    pub kind: String,
    /// As written in the pack.
    pub value: String,
    /// First day it applies.
    pub effective_from: String,
    /// Last day, if it ends.
    pub effective_to: Option<String>,
    /// Act and provision.
    pub citation: String,
    /// Where to read the act.
    pub url: String,
    /// Context.
    pub note: Option<String>,
}

/// The rule pack in force.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RulePackDto {
    /// `cz-2026@2026.1`.
    pub provenance: String,
    /// `draft` or `reviewed`.
    pub review: String,
    /// Scope.
    pub summary: String,
    /// What the pack leaves out, and why.
    pub omitted: Vec<String>,
    /// Values effective on the entity's as-of date.
    pub values: Vec<PackValueDto>,
    /// Public holidays it knows.
    pub holidays: u32,
}

/// One run in the egress register.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EgressRunDto {
    /// Stable id.
    pub id: String,
    /// RFC 3339 UTC.
    pub at: String,
    /// Which advisor.
    pub advisor: String,
    /// What for.
    pub purpose: String,
    /// The provider, e.g. the user's Claude Code installation.
    pub provider: String,
    /// The model setting used.
    pub model: String,
    /// What left the machine, summarised.
    pub sent: String,
    /// Fields removed by the egress gate.
    pub redacted: Vec<String>,
    /// MCP tool calls answered.
    pub tool_calls: u32,
    /// Bytes sent, prompts and tool results together.
    pub bytes_sent: u32,
    /// What came of it.
    pub outcome: String,
}
