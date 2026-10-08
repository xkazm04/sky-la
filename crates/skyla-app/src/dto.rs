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

/// A bank statement line in the workbench.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BankLineDto {
    /// Stable id: `s<import>-<line>`.
    pub id: String,
    /// Booking date.
    pub date: String,
    /// Who (or the bank's text when no party is named).
    pub counterparty: String,
    /// The other party's account, canonical.
    pub counterparty_account: Option<String>,
    /// Symbols and message.
    pub reference: String,
    /// Signed amount.
    pub amount: MoneyDto,
    /// `certain` (accepted with one press), `needs_you`, or `booked`.
    pub status: String,
    /// What it was booked as.
    pub booked_as: Option<String>,
    /// The entry it was booked as.
    pub entry_id: Option<i64>,
    /// What the workbench proposes, e.g. `Settle 2026-102`.
    pub proposal: Option<String>,
    /// Why it isn't accepted with one press.
    pub held_because: Option<String>,
    /// An inbox proposal about the line.
    pub proposal_id: Option<String>,
    /// Scored candidates.
    pub candidates: Vec<MatchCandidateDto>,
}

/// One imported file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BankImportDto {
    /// File name.
    pub file: String,
    /// First day.
    pub from: String,
    /// Last day.
    pub to: String,
    /// New lines it brought.
    pub lines: u32,
    /// Its closing balance.
    pub closing: Option<MoneyDto>,
}

/// A bank rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BankRuleDto {
    /// Id.
    pub id: u32,
    /// Name.
    pub name: String,
    /// When and what, in words.
    pub summary: String,
    /// Accepted with the certain ones.
    pub auto_accept: bool,
}

/// A rule as the user creates it from a line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BankRuleInputDto {
    /// Name.
    pub name: String,
    /// The account to book to.
    pub account: String,
    /// An input VAT code, when the payments carry VAT.
    pub vat_code: Option<String>,
    /// Accept future lines it fits with the certain ones.
    pub auto_accept: bool,
}

/// One part of how a line is booked: an invoice to settle (by its ledger
/// entry) or an account row with an optional VAT code. Amounts as typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BankAllocationDto {
    /// The invoice's ledger entry.
    pub entry_id: Option<i64>,
    /// Or an account.
    pub account: Option<String>,
    /// Input VAT code for an account row.
    pub vat_code: Option<String>,
    /// Gross amount, e.g. `3 630,00`.
    pub amount: String,
}

/// The bank workbench.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BankStatementDto {
    /// `221 · ČSOB Business ··4412`.
    pub account_name: String,
    /// The latest file.
    pub file: String,
    /// Its format.
    pub format: String,
    /// Its first day.
    pub from: String,
    /// Its last day.
    pub to: String,
    /// Its opening balance.
    pub opening: MoneyDto,
    /// Money in.
    pub credits: MoneyDto,
    /// Money out.
    pub debits: MoneyDto,
    /// Opening + lines.
    pub closing: MoneyDto,
    /// What the bank reports.
    pub reported_closing: MoneyDto,
    /// Always true: a statement that doesn't tie out is refused at import.
    pub ties_out: bool,
    /// Every import, oldest first.
    pub imports: Vec<BankImportDto>,
    /// Every line, newest first.
    pub lines: Vec<BankLineDto>,
    /// The user's rules.
    pub rules: Vec<BankRuleDto>,
    /// Accounts a line can be booked to.
    pub accounts: Vec<AccountChoiceDto>,
    /// Input VAT codes a purchase can carry.
    pub vat_codes: Vec<VatCodeChoiceDto>,
}

/// An account the user can pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountChoiceDto {
    /// Code.
    pub code: String,
    /// Name.
    pub name: String,
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

/// A rendered document, ready to save or open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPdfDto {
    /// Suggested file name, e.g. `Faktura 2026-041.pdf`.
    pub file_name: String,
    /// The PDF, base64-encoded.
    pub pdf_base64: String,
    /// Pages in the PDF.
    pub pages: u32,
    /// The QR Platba payload, if the document asks for a payment.
    pub spayd: Option<String>,
}

/// A document written in an exchange format (ISDOC, UBL, CII).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentXmlDto {
    /// Suggested file name, e.g. `2026-041.isdoc`.
    pub file_name: String,
    /// The file's media type.
    pub media_type: String,
    /// The XML.
    pub xml: String,
}

/// Statutory late interest on one receivable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LateInterestDto {
    /// The first day of delay.
    pub delay_from: String,
    /// The half-year start whose ČNB repo rate applies.
    pub rate_date: String,
    /// Annual percent, e.g. `11.5`.
    pub annual_rate: String,
    /// One period per stretch at a constant principal.
    pub periods: Vec<InterestPeriodDto>,
    /// Sum of the periods.
    pub total: MoneyDto,
    /// The statutory minimum recovery cost.
    pub recovery_cost: MoneyDto,
}

/// A stretch of delay at a constant principal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InterestPeriodDto {
    /// First day.
    pub from: String,
    /// Last day, inclusive.
    pub to: String,
    /// Days.
    pub days: i64,
    /// What was owed.
    pub principal: MoneyDto,
    /// Interest.
    pub interest: MoneyDto,
}

/// A reminder that went out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReminderSentDto {
    /// The step (1 friendly, then firmer).
    pub step: u8,
    /// When it was recorded as sent.
    pub sent_on: String,
}

/// A reminder that is due, drafted for the user to send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DunningNoticeDto {
    /// The invoice.
    pub document_id: i64,
    /// Its number.
    pub number: String,
    /// Who owes.
    pub customer: String,
    /// The step, from 1.
    pub step: u8,
    /// `friendly`, `firm` or `final`.
    pub tone: String,
    /// When the invoice was due.
    pub due_on: String,
    /// When this step became due.
    pub scheduled_on: String,
    /// Days past due.
    pub days_overdue: i64,
    /// Still owed.
    pub open: MoneyDto,
    /// The late interest stated on the final step.
    pub interest: Option<LateInterestDto>,
    /// Why the interest is missing (no ČNB rate history imported).
    pub interest_problem: Option<String>,
    /// Draft subject, Czech.
    pub subject_cs: String,
    /// Draft body, Czech.
    pub body_cs: String,
    /// Draft subject, English.
    pub subject_en: String,
    /// Draft body, English.
    pub body_en: String,
}

/// A recurring invoice template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecurringTemplateDto {
    /// Row id.
    pub id: i64,
    /// For people.
    pub name: String,
    /// The customer.
    pub client: String,
    /// `weekly`, `monthly`, `quarterly` or `yearly`.
    pub frequency: String,
    /// Units between runs.
    pub interval: u32,
    /// The first occurrence.
    pub start: String,
    /// The next occurrence, if the schedule hasn't ended.
    pub next: Option<String>,
    /// Days to the due date.
    pub due_days: u16,
    /// Issued automatically, or left as a draft.
    pub auto_issue: bool,
    /// Paused templates don't run.
    pub active: bool,
    /// Each invoice's total, with VAT, at today's pack rates.
    pub gross: MoneyDto,
}

/// What the invoice editor offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceFormDto {
    /// Customers seen on earlier documents.
    pub clients: Vec<ClientDto>,
    /// The VAT codes this supplier may put on a sale.
    pub vat_codes: Vec<VatCodeChoiceDto>,
    /// Units the line editor suggests.
    pub units: Vec<String>,
    /// Payment terms the editor offers, in days.
    pub due_days: Vec<u16>,
    /// Today: the issue date a draft issued now gets.
    pub today: String,
    /// The number the next invoice issued today gets.
    pub next_number: String,
}

/// A customer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClientDto {
    /// Legal name.
    pub name: String,
    /// IČO.
    pub ico: Option<String>,
    /// DIČ.
    pub dic: Option<String>,
    /// Postal address.
    pub address: Option<String>,
}

/// A VAT code the editor offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatCodeChoiceDto {
    /// The code.
    pub code: String,
    /// What it means.
    pub name: String,
    /// The rate today, e.g. `21`.
    pub rate_percent: String,
}

/// A draft invoice as typed in the editor. The core parses and checks it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceDraftDto {
    /// A customer's legal name, as [`InvoiceFormDto::clients`] lists them;
    /// empty for a new customer.
    pub client: String,
    /// A customer not on any document yet, when `client` is empty. Saving
    /// the draft adds them to the list.
    pub new_client: Option<ClientDto>,
    /// Days from today to the due date.
    pub due_days: u16,
    /// Free text printed on the invoice.
    pub note: String,
    /// The lines.
    pub lines: Vec<InvoiceDraftLineDto>,
}

/// A line as typed: Czech number formats (`1,5`, `1 200,00`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceDraftLineDto {
    /// What was supplied.
    pub description: String,
    /// Quantity, e.g. `21` or `1,5`.
    pub quantity: String,
    /// Unit, e.g. `h`.
    pub unit: String,
    /// Price per unit excluding VAT, e.g. `1 200,00`.
    pub unit_price: String,
    /// One of [`InvoiceFormDto::vat_codes`].
    pub vat_code: String,
}

/// Reference data and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RefDataDto {
    /// Whether the user allows fetching from the ČNB.
    pub fetch_enabled: bool,
    /// The only host fetches go to.
    pub fetch_host: String,
    /// Days of exchange rates loaded.
    pub fx_days: u32,
    /// Today's euro rate, when loaded.
    pub euro: Option<String>,
    /// Repo-rate changes loaded.
    pub repo_changes: u32,
    /// The repo rate in force today, when loaded.
    pub repo_now: Option<String>,
    /// Every import or fetch, oldest first.
    pub sources: Vec<RefSourceDto>,
    /// Keys trusted to sign pack updates.
    pub trusted_keys: u32,
    /// A verified pack update waiting for the next opening.
    pub pending_pack: Option<String>,
}

/// One import or fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RefSourceDto {
    /// `cnb_fx` or `cnb_repo`.
    pub kind: String,
    /// `imported <file>` or `fetched <url>`.
    pub origin: String,
    /// What it brought.
    pub summary: String,
    /// When.
    pub on: String,
}

/// A verified pack update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PackUpdateDto {
    /// The pack in use.
    pub in_use: String,
    /// The verified update.
    pub installed: String,
    /// For people.
    pub message: String,
}

/// The full export, as one zip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportDto {
    /// Suggested file name.
    pub file_name: String,
    /// Files inside.
    pub files: u32,
    /// Its size.
    pub bytes: u32,
    /// The zip, base64.
    pub content_base64: String,
}

/// One document in an invoice import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportDocumentDto {
    /// Where in the file (item or row).
    pub position: u32,
    /// Its number.
    pub number: String,
    /// `invoice` or `credit_note`.
    pub kind: String,
    /// Issued.
    pub issue_date: String,
    /// The customer's name.
    pub customer: String,
    /// Base.
    pub base: MoneyDto,
    /// VAT.
    pub vat: MoneyDto,
    /// Total.
    pub total: MoneyDto,
    /// `new`, `duplicate` or `problem`.
    pub status: String,
    /// Why it isn't new.
    pub problems: Vec<String>,
}

/// What an invoice import holds, or did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewDto {
    /// File name.
    pub file: String,
    /// `Pohoda` or `Fakturoid`.
    pub source: String,
    /// Documents that would be posted.
    pub new: u32,
    /// Every document read.
    pub documents: Vec<ImportDocumentDto>,
    /// What in the file couldn't be read.
    pub problems: Vec<String>,
    /// After a commit: the numbers posted.
    pub imported: Vec<String>,
}

/// A newer release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDto {
    /// Its version.
    pub version: String,
    /// When it was published.
    pub published: String,
    /// What changed, briefly.
    pub notes: String,
    /// Its release page.
    pub page: String,
}

/// The opt-in update check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatusDto {
    /// Whether the user turned it on.
    pub enabled: bool,
    /// This build's version.
    pub current: String,
    /// The only host it asks.
    pub source: String,
    /// Release keys this build trusts; none means no check is made.
    pub trusted_keys: u32,
    /// Whether a check ran since it was turned on.
    pub checked: bool,
    /// A newer release, if the last check found one.
    pub available: Option<UpdateDto>,
}

/// One line of a received invoice as typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseLineDraftDto {
    /// What it was for.
    pub description: String,
    /// The expense account.
    pub account: String,
    /// An input-VAT code, or none when no VAT is deducted.
    pub vat_code: Option<String>,
    /// The amount without VAT, in Czech format (`1 200,00`).
    pub base: String,
}

/// A received invoice as typed in the editor. The core checks it and
/// computes the VAT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseDraftDto {
    /// The supplier's legal name.
    pub supplier: String,
    /// Their IČO.
    pub ico: Option<String>,
    /// Their DIČ; needed to deduct VAT.
    pub dic: Option<String>,
    /// The invoice's number, as the supplier printed it.
    pub number: String,
    /// Issued.
    pub issue_date: String,
    /// DUZP; the issue date when empty.
    pub tax_point_date: Option<String>,
    /// Due.
    pub due_date: Option<String>,
    /// The lines.
    pub lines: Vec<PurchaseLineDraftDto>,
    /// The VAT the invoice states, to check against the computed VAT.
    pub stated_vat: Option<String>,
}

/// What the purchase editor offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseFormDto {
    /// Expense accounts.
    pub accounts: Vec<AccountChoiceDto>,
    /// Input-VAT codes; none when the books aren't VAT-registered.
    pub vat_codes: Vec<VatCodeChoiceDto>,
    /// Whether these books deduct VAT.
    pub vat_payer: bool,
    /// Today.
    pub today: String,
}

/// A received invoice in the books.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseDto {
    /// Its journal entry.
    pub entry_id: i64,
    /// The supplier's number.
    pub number: String,
    /// The supplier.
    pub supplier: String,
    /// Their DIČ.
    pub dic: Option<String>,
    /// The entry's date (the tax point).
    pub issued_on: String,
    /// Due.
    pub due_on: Option<String>,
    /// Without VAT.
    pub base: MoneyDto,
    /// VAT deducted.
    pub vat: MoneyDto,
    /// Owed in all.
    pub gross: MoneyDto,
    /// Paid so far.
    pub paid: MoneyDto,
    /// Still owed.
    pub open: MoneyDto,
    /// `open`, `overdue` or `paid`.
    pub status: String,
    /// Days past due, when overdue.
    pub days_overdue: Option<i64>,
}

/// A period a screen can report on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PeriodChoiceDto {
    /// `2026-09`, `2026-Q3`, `2026-ytd`.
    pub id: String,
    /// `September 2026`, `Q3 2026`, `Apr – Sep 2026`.
    pub label: String,
    /// First day.
    pub from: String,
    /// Last day.
    pub to: String,
}

/// What the screens report on, as of the books' date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReportingPeriodsDto {
    /// The books' date.
    pub today: String,
    /// Its year, for the obligations calendar.
    pub year: i64,
    /// Where the books begin.
    pub books_from: String,
    /// The last VAT periods that have ended, newest first; none when the
    /// books aren't VAT-registered.
    pub vat: Vec<PeriodChoiceDto>,
    /// Up to two ended quarters that overlap the books, oldest first.
    pub quarters: Vec<PeriodChoiceDto>,
    /// The last of them.
    pub last_quarter: Option<PeriodChoiceDto>,
    /// The one before, to compare with.
    pub prior_quarter: Option<PeriodChoiceDto>,
    /// The last quarter's year up to its end (or the books so far).
    pub year_to_date: PeriodChoiceDto,
}

/// A recurring invoice as typed in the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecurringDraftDto {
    /// For people; the customer's name when empty.
    pub name: String,
    /// The invoice each occurrence produces; `dueDays` counts from each issue.
    pub draft: InvoiceDraftDto,
    /// `monthly`, `quarterly` or `yearly`.
    pub frequency: String,
    /// Every how many of them, 1 to 12.
    pub interval: u32,
    /// The first occurrence; it anchors the day of the month.
    pub start: String,
    /// Issue each occurrence instead of leaving a draft.
    pub auto_issue: bool,
}

/// Whether there are books to unlock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionStateDto {
    /// `needs_setup`, `locked`, `open` or `demo`.
    pub state: String,
    /// Whose books, when known.
    pub entity: Option<String>,
    /// The OS keychain holds the key, so unlocking needn't ask.
    pub remembered: bool,
}

/// A recovery key to show once and have confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKeyDto {
    /// In groups of four, e.g. `ABCD-EFGH-…`.
    pub key: String,
    /// How many groups.
    pub groups: u32,
}

/// The first-run form: who the books are for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EntitySetupDto {
    /// The name on invoices (an OSVČ's own name, or a trade name).
    pub display_name: String,
    /// IČO (may be empty while registering).
    pub ico: String,
    /// DIČ, for VAT payers.
    pub dic: Option<String>,
    /// The address printed on invoices.
    pub address: String,
    /// `monthly`, `quarterly` or `none`.
    pub vat_period: String,
    /// The trade-register line printed on invoices.
    pub registration: String,
    /// The business account's IBAN.
    pub iban: Option<String>,
    /// The bank's name, for the status line.
    pub bank_name: String,
    /// An email for invoices.
    pub email: Option<String>,
    /// `craft`, `trade` or `liberal`, when known.
    pub flat_rate_group: Option<String>,
}

/// One backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupDto {
    /// The file.
    pub file: String,
    /// When (UTC).
    pub created_at: String,
    /// Size.
    pub bytes: u32,
    /// The journal's chain head it anchors.
    pub chain_head: Option<String>,
}

/// The backups and the policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupsDto {
    /// The demo keeps its books in memory and has none.
    pub demo: bool,
    /// Where they go.
    pub folder: Option<String>,
    /// Every this many days.
    pub every_days: u32,
    /// How many are kept.
    pub keep: u32,
    /// Newest first.
    pub backups: Vec<BackupDto>,
}

/// What the restore drill found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DrillDto {
    /// The backup checked.
    pub file: String,
    /// Everything checked out.
    pub passed: bool,
    /// The file is the one its manifest describes.
    pub file_matches: bool,
    /// It opened with the key.
    pub opens: bool,
    /// Its content matches the manifest.
    pub content_matches: bool,
    /// Its chain head matches the manifest.
    pub chain_matches: Option<bool>,
}

/// What to explain: an account over a period, or one entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplainTargetDto {
    /// `account` or `entry`.
    pub kind: String,
    /// For an account.
    pub account: Option<String>,
    /// First day, for an account.
    pub from: Option<String>,
    /// Last day, for an account.
    pub to: Option<String>,
    /// For an entry.
    pub entry: Option<i64>,
}

/// An entry behind a figure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplainedEntryDto {
    /// Entry id.
    pub id: i64,
    /// Date.
    pub date: String,
    /// Memo.
    pub memo: String,
    /// Its amount on the account (or its gross, for an entry).
    pub amount: MoneyDto,
}

/// An explanation, checked against the books.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplanationDto {
    /// `accepted`, `rejected`, `needs_confirmation`, `blocked`, `failed`.
    pub status: String,
    /// The explanation, when accepted.
    pub text: Option<String>,
    /// Entries it cites, when accepted.
    pub cites: Vec<i64>,
    /// Why it was rejected or didn't run.
    pub problems: Vec<String>,
    /// Figures matched to the books.
    pub grounded: u32,
    /// The register entry.
    pub run_id: Option<String>,
    /// Every entry behind the figure.
    pub entries: Vec<ExplainedEntryDto>,
}

/// A detector's finding, citing the entries it rests on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FindingDto {
    /// Stable id.
    pub id: String,
    /// `variance`, `vendor_rate`, `client_margin`, `late_payer`, `subscription`, `runway`.
    pub detector: String,
    /// One line.
    pub title: String,
    /// Second line.
    pub detail: String,
    /// The period it's about.
    pub period: String,
    /// Its figures.
    pub figures: Vec<(String, MoneyDto)>,
    /// Its percentages, whole percent.
    pub percents: Vec<(String, String)>,
    /// Journal entries it rests on.
    pub cites: Vec<i64>,
}

/// The tax advisor's answer, checked against the engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaxAdviceDto {
    /// `accepted`, `rejected`, `needs_confirmation`, `blocked`, or how the run failed.
    pub status: String,
    /// The scenario it puts first, when accepted.
    pub recommendation: Option<String>,
    /// Its explanation, when accepted (every figure checked).
    pub explanation: Option<String>,
    /// Levers it considered.
    pub levers: Vec<String>,
    /// Facts it needs from the user.
    pub questions: Vec<String>,
    /// Why it was rejected or didn't run.
    pub problems: Vec<String>,
    /// Figures matched to engine values.
    pub grounded: u32,
    /// The register entry.
    pub run_id: Option<String>,
    /// The engine's scenarios it explains.
    pub scenarios: TaxScenariosDto,
}

/// Whether advisors can run, and what to do if not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorStatusDto {
    /// `claude-code-cli` or `fake`.
    pub provider: String,
    /// `ready`, `not_installed`, `not_signed_in` or `rate_limited`.
    pub state: String,
    /// What the provider reports about itself, when ready.
    pub version: Option<String>,
    /// Where the CLI was looked for, when it wasn't found.
    pub looked_in: Vec<String>,
    /// When a usage limit resets (UTC, RFC 3339), if known.
    pub resets_at: Option<String>,
    /// Runs replay recordings; no model is called.
    pub demo: bool,
}

/// One deadline in the obligations calendar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ObligationDto {
    /// The pack's obligation id.
    pub obligation: String,
    /// For people.
    pub name: String,
    /// `file`, `pay` or `file_and_pay`.
    pub action: String,
    /// `2026-09`, `2026-Q3` or `2025`.
    pub period: String,
    /// The date the rule gives.
    pub nominal: String,
    /// The deadline, after any shift.
    pub due: String,
    /// It moved past a weekend or holiday.
    pub shifted: bool,
    /// Act and provision.
    pub citation: String,
    /// `past`, `next` or `upcoming`, against the as-of date.
    pub status: String,
}

/// The user's projection for the year, typed in the scenario form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaxProjectionDto {
    /// § 7 income, e.g. `1 571 000`.
    pub income: String,
    /// Actual tax-deductible expenses.
    pub expenses: String,
    /// `craft`, `trade` or `liberal`; empty when unknown.
    pub flat_rate: Option<String>,
    /// A planned purchase, for the timing lever (may be empty).
    pub purchase_description: String,
    /// Its price excluding claimable VAT (empty for none).
    pub purchase_price: String,
}

/// The § 7 scenarios for the year.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaxScenariosDto {
    /// The tax year.
    pub year: String,
    /// `books` (cash basis so far) or `projection`.
    pub source: String,
    /// First day covered.
    pub from: String,
    /// Last day covered.
    pub to: String,
    /// § 7 income.
    pub income: MoneyDto,
    /// Actual expenses.
    pub actual_expenses: MoneyDto,
    /// The flat-rate group, when known.
    pub flat_rate: Option<String>,
    /// The scenario the others are compared with.
    pub baseline: String,
    /// The scenario with the lowest tax and insurance.
    pub lowest_total: String,
    /// Every combination.
    pub scenarios: Vec<TaxScenarioDto>,
    /// Facts that would change the analysis.
    pub questions: Vec<String>,
    /// Levers that couldn't be evaluated, and why.
    pub not_evaluated: Vec<String>,
    /// What the computation assumed.
    pub assumptions: Vec<String>,
    /// Pack provenance.
    pub pack: String,
}

/// One scenario's worksheet and its difference from the baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaxScenarioDto {
    /// Stable id.
    pub id: String,
    /// For people.
    pub label: String,
    /// Expenses claimed.
    pub expenses: MoneyDto,
    /// The flat rate, when used (`60`).
    pub flat_rate_percent: Option<String>,
    /// Its cap.
    pub flat_rate_cap: Option<MoneyDto>,
    /// The cap applied.
    pub capped: bool,
    /// Income minus expenses.
    pub profit: MoneyDto,
    /// Rounded tax base.
    pub tax_base: MoneyDto,
    /// Rate (`15`).
    pub tax_rate_percent: String,
    /// Tax before credits.
    pub tax_before_credits: MoneyDto,
    /// Taxpayer credit used.
    pub taxpayer_credit: MoneyDto,
    /// Income tax.
    pub tax: MoneyDto,
    /// Social insurance assessment base (also the pension base).
    pub social_base: MoneyDto,
    /// Its rate.
    pub social_rate_percent: String,
    /// Social insurance.
    pub social: MoneyDto,
    /// Health insurance assessment base.
    pub health_base: MoneyDto,
    /// Its rate.
    pub health_rate_percent: String,
    /// Health insurance.
    pub health: MoneyDto,
    /// Tax and insurance.
    pub total: MoneyDto,
    /// Tax against the baseline.
    pub vs_baseline_tax: MoneyDto,
    /// Both insurances against the baseline.
    pub vs_baseline_insurance: MoneyDto,
    /// Everything against the baseline.
    pub vs_baseline_total: MoneyDto,
    /// The pension assessment base against the baseline.
    pub vs_baseline_pension_base: MoneyDto,
}

/// The kontrolní hlášení for a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ControlStatementDto {
    /// First day.
    pub from: String,
    /// Last day.
    pub to: String,
    /// Documents above this (VAT included) are itemised.
    pub threshold: MoneyDto,
    /// Services received from the EU.
    pub a2: Vec<KhItemDto>,
    /// Supplies to VAT payers above the threshold.
    pub a4: Vec<KhItemDto>,
    /// Every other supply.
    pub a5: KhTotalsDto,
    /// Purchases from VAT payers above the threshold.
    pub b2: Vec<KhItemDto>,
    /// Every other purchase.
    pub b3: KhTotalsDto,
    /// Section C against the DPH return's rows.
    pub c: Vec<KhCRowDto>,
    /// Every section C row equals the return's.
    pub matches_return: bool,
    /// Documents that couldn't be placed.
    pub problems: Vec<String>,
    /// The filing deadline from the pack.
    pub due_on: String,
    /// Pack provenance.
    pub pack: String,
}

/// The souhrnné hlášení (EC Sales List) for a period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecapitulativeStatementDto {
    /// First day.
    pub from: String,
    /// Last day.
    pub to: String,
    /// Per customer VAT number and supply code.
    pub lines: Vec<RecapitulativeLineDto>,
    /// Sum of the lines' bases.
    pub total: MoneyDto,
    /// Documents that couldn't be placed.
    pub problems: Vec<String>,
    /// Pack provenance.
    pub pack: String,
}

/// One customer and supply code on the souhrnné hlášení.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecapitulativeLineDto {
    /// The customer's country prefix.
    pub country: String,
    /// Their VAT number without the prefix.
    pub vat_number: String,
    /// The supply code (kód plnění): `0` goods, `3` services.
    pub sh_code: String,
    /// The customer's name.
    pub counterparty: String,
    /// The supplies' base.
    pub base: MoneyDto,
    /// How many supplies.
    pub supplies: u32,
}

/// An itemised line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KhItemDto {
    /// Document number.
    pub number: String,
    /// The other party.
    pub counterparty: String,
    /// Their VAT id.
    pub vat_id: String,
    /// DUZP or DPPD.
    pub date: String,
    /// Base, standard rate.
    pub base_standard: MoneyDto,
    /// Tax, standard rate.
    pub tax_standard: MoneyDto,
    /// Base, reduced rate.
    pub base_reduced: MoneyDto,
    /// Tax, reduced rate.
    pub tax_reduced: MoneyDto,
    /// Base at both rates.
    pub base: MoneyDto,
    /// Tax at both rates.
    pub tax: MoneyDto,
}

/// A summary section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KhTotalsDto {
    /// Base, standard rate.
    pub base_standard: MoneyDto,
    /// Tax, standard rate.
    pub tax_standard: MoneyDto,
    /// Base, reduced rate.
    pub base_reduced: MoneyDto,
    /// Tax, reduced rate.
    pub tax_reduced: MoneyDto,
    /// Documents counted.
    pub documents: u32,
}

/// One section C row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KhCRowDto {
    /// The return row.
    pub row: String,
    /// What the statement totals.
    pub base: MoneyDto,
    /// What the return says.
    pub return_base: MoneyDto,
    /// They agree.
    pub matches: bool,
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
    /// The provider's own cost estimate, when it gave one.
    pub cost_note: Option<String>,
    /// The register's hash chain is unbroken up to and including this run.
    pub intact: bool,
}

/// Exactly what one run sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EgressPayloadDto {
    /// The run.
    pub id: String,
    /// Bytes sent.
    pub bytes: u32,
    /// The run's link in the register's chain.
    pub hash: String,
    /// The payload, pretty-printed when it's JSON.
    pub text: String,
}

/// A task type, what it may send, and the user's policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EgressPolicyDto {
    /// Task id.
    pub task: String,
    /// Which advisor.
    pub advisor: String,
    /// For people.
    pub label: String,
    /// What it may send.
    pub scope: Vec<String>,
    /// `always`, `ask` or `never`.
    pub policy: String,
}
