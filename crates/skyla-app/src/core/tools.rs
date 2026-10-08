//! The tools advisors may call (WP-25), and nothing else: each one reads,
//! computes with the engine, or files a proposal for a person to review.
//! None of them writes to the ledger: a proposed entry is checked by the
//! kernel in a transaction that's always rolled back, and kept in memory
//! until someone approves it in the inbox.

use serde::Serialize;
use serde_json::{Value, json};

use super::Core;
use crate::demo::{DomainEntry, DomainEntryLine};
use crate::dto::{ProposalDto, TaxProjectionDto};
use crate::error::CoreError;

/// What a tool may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    /// Returns figures from the books or the pack.
    Read,
    /// Runs the engine on given inputs.
    Compute,
    /// Files something for a person to review.
    Propose,
}

/// A tool as the MCP shim lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolSpec {
    /// Name, without the `mcp__skyla__` prefix.
    pub name: &'static str,
    /// What it may do.
    pub kind: ToolKind,
    /// For the model.
    pub description: &'static str,
    /// JSON Schema of the arguments.
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

fn range_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "from": { "type": "string", "format": "date" },
            "to": { "type": "string", "format": "date" }
        },
        "required": ["from", "to"],
        "additionalProperties": false
    })
}

/// Every tool, in a stable order.
pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "get_period_summary",
            kind: ToolKind::Read,
            description: "Revenue, expenses and profit for a date range, in minor units, with the ledger snapshot they come from.",
            input_schema: range_schema(),
        },
        ToolSpec {
            name: "get_vat_return",
            kind: ToolKind::Read,
            description: "The DPH return rows for a period as the rule pack maps them, with the payable amount and the deadline.",
            input_schema: range_schema(),
        },
        ToolSpec {
            name: "get_cash_basis",
            kind: ToolKind::Read,
            description: "Taxable income and deductible expenses on the cash basis (daňová evidence) for a date range.",
            input_schema: range_schema(),
        },
        ToolSpec {
            name: "list_unmatched_bank_lines",
            kind: ToolKind::Read,
            description: "Bank lines the matcher couldn't place: date, counterparty, payment message and amount.",
            input_schema: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolSpec {
            name: "get_rule_value",
            kind: ToolKind::Read,
            description: "A statutory value from the rule pack in force on a date, with its citation.",
            input_schema: json!({
                "type": "object",
                "properties": { "key": { "type": "string" }, "on": { "type": "string", "format": "date" } },
                "required": ["key", "on"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "list_obligations",
            kind: ToolKind::Read,
            description: "Every filing and payment deadline in a year, from the rule pack.",
            input_schema: json!({
                "type": "object",
                "properties": { "year": { "type": "integer" } },
                "required": ["year"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "run_scenario",
            kind: ToolKind::Compute,
            description: "The § 7 scenarios (actual vs flat-rate expenses, purchase timing): tax, insurance and side effects computed by the engine. Without a projection, from the books so far.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "projection": {
                        "type": ["object", "null"],
                        "properties": {
                            "income": { "type": "string" },
                            "expenses": { "type": "string" },
                            "flatRate": { "type": ["string", "null"], "enum": ["craft", "trade", "liberal", null] },
                            "purchaseDescription": { "type": "string" },
                            "purchasePrice": { "type": "string" }
                        },
                        "required": ["income", "expenses", "purchaseDescription", "purchasePrice"]
                    }
                },
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "propose_entry",
            kind: ToolKind::Propose,
            description: "Propose a journal entry for the user to review. The kernel checks it (balance, open period, leaf accounts); nothing is posted.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string", "format": "date" },
                    "memo": { "type": "string" },
                    "reason": { "type": "string" },
                    "lines": {
                        "type": "array",
                        "minItems": 2,
                        "items": {
                            "type": "object",
                            "properties": {
                                "account": { "type": "string" },
                                "amount_minor": { "type": "integer", "description": "Debit positive, credit negative." },
                                "vat_code": { "type": ["string", "null"] }
                            },
                            "required": ["account", "amount_minor"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["date", "memo", "reason", "lines"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "propose_categorisation",
            kind: ToolKind::Propose,
            description: "Suggest an account for a bank line the matcher couldn't place, with the reason. The user books it.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "line": { "type": "string" },
                    "account": { "type": "string" },
                    "reason": { "type": "string" }
                },
                "required": ["line", "account", "reason"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "propose_finding",
            kind: ToolKind::Propose,
            description: "File a finding for the user's review. It must cite the engine values or entries it rests on.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "detail": { "type": "string" },
                    "cites": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
                },
                "required": ["title", "detail", "cites"],
                "additionalProperties": false
            }),
        },
        ToolSpec {
            name: "ask_user",
            kind: ToolKind::Propose,
            description: "Ask the user for a fact the books can't contain. It becomes an inbox item.",
            input_schema: json!({
                "type": "object",
                "properties": { "question": { "type": "string" }, "why": { "type": "string" } },
                "required": ["question", "why"],
                "additionalProperties": false
            }),
        },
    ]
}

/// A tool call the core refused, with the reason the model sees.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ToolError(pub String);

impl From<CoreError> for ToolError {
    fn from(e: CoreError) -> Self {
        Self(e.to_string())
    }
}

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError(format!("{key} is required and must be a string")))
}

fn to_json<T: Serialize>(value: &T) -> Result<Value, ToolError> {
    serde_json::to_value(value).map_err(|e| ToolError(e.to_string()))
}

impl Core {
    fn file_proposal(&self, p: ProposalDto) -> Result<Value, ToolError> {
        self.file_entry(p, None)
    }

    /// Files a proposal with the entry it proposes, kept as proposed.
    fn file_entry(&self, p: ProposalDto, entry: Option<DomainEntry>) -> Result<Value, ToolError> {
        let id = self.inbox().file(p, entry);
        self.persist_inbox()?;
        Ok(json!({ "proposal": id, "status": "waiting for the user's review" }))
    }

    /// Calls one tool. Unknown tools and bad arguments are refused with a
    /// reason; nothing a tool does reaches the ledger.
    pub fn call_tool(&self, name: &str, args: &Value) -> Result<Value, ToolError> {
        match name {
            "get_period_summary" => {
                let pl = self.profit_and_loss(text(args, "from")?, text(args, "to")?)?;
                to_json(&pl)
            }
            "get_vat_return" => to_json(&self.vat_return(text(args, "from")?, text(args, "to")?)?),
            "get_cash_basis" => {
                let cb = self.cash_basis(text(args, "from")?, text(args, "to")?)?;
                Ok(json!({
                    "from": cb.from, "to": cb.to,
                    "taxableIncome": cb.taxable_income,
                    "deductibleExpenses": cb.deductible_expenses,
                    "totals": cb.totals,
                }))
            }
            "list_unmatched_bank_lines" => {
                let lines: Vec<Value> = self
                    .bank_statement()?
                    .lines
                    .into_iter()
                    .filter(|l| l.status == "needs_you")
                    .map(|l| {
                        json!({
                            "line": l.id,
                            "date": l.date,
                            "counterparty": l.counterparty,
                            "counterpartyAccount": l.counterparty_account,
                            "reference": l.reference,
                            "amount": l.amount,
                        })
                    })
                    .collect();
                Ok(json!({ "lines": lines }))
            }
            "get_rule_value" => {
                let (key, on) = (text(args, "key")?, text(args, "on")?);
                let v = self
                    .pack
                    .value(key, on)
                    .map_err(|e| ToolError(e.to_string()))?;
                let act = self
                    .pack
                    .acts
                    .get(&v.cite.act)
                    .map_or("", |a| a.name.as_str());
                Ok(json!({
                    "key": key,
                    "value": v.value,
                    "kind": v.kind.name(),
                    "citation": format!("{act}, {}", v.cite.section),
                    "pack": self.pack.provenance(),
                }))
            }
            "list_obligations" => {
                let year = args
                    .get("year")
                    .and_then(Value::as_i64)
                    .and_then(|y| i32::try_from(y).ok())
                    .ok_or_else(|| ToolError("year must be an integer".into()))?;
                to_json(&self.obligations(year)?)
            }
            "run_scenario" => {
                let projection: Option<TaxProjectionDto> = match args.get("projection") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(
                        serde_json::from_value(v.clone()).map_err(|e| ToolError(e.to_string()))?,
                    ),
                };
                to_json(&self.income_tax_scenarios(projection.as_ref())?)
            }
            "propose_entry" => self.propose_entry(args),
            "propose_categorisation" => self.propose_categorisation(args),
            "propose_finding" => {
                let cites: Vec<String> = args
                    .get("cites")
                    .and_then(Value::as_array)
                    .map(|c| {
                        c.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                if cites.is_empty() {
                    return Err(ToolError("a finding must cite what it rests on".into()));
                }
                self.file_proposal(ProposalDto {
                    id: String::new(),
                    kind: "advice".into(),
                    title: text(args, "title")?.to_owned(),
                    detail: text(args, "detail")?.to_owned(),
                    confidence: None,
                    source_kind: "advisor".into(),
                    source: "Advisor".into(),
                    bank_line_id: None,
                    due_on: None,
                    amount: None,
                    entry: None,
                    reasons: cites.iter().map(|c| format!("Cites {c}")).collect(),
                })
            }
            "ask_user" => self.file_proposal(ProposalDto {
                id: String::new(),
                kind: "advice".into(),
                title: text(args, "question")?.to_owned(),
                detail: "An advisor needs this to finish · answer when you can".into(),
                confidence: Some("needs_you".into()),
                source_kind: "advisor".into(),
                source: "Advisor question".into(),
                bank_line_id: None,
                due_on: None,
                amount: None,
                entry: None,
                reasons: vec![text(args, "why")?.to_owned()],
            }),
            other => Err(ToolError(format!("there is no tool {other}"))),
        }
    }

    /// An account for a bank line the matcher couldn't place: filed as the
    /// entry that books the line there, for the user to approve.
    fn propose_categorisation(&self, args: &Value) -> Result<Value, ToolError> {
        let (line, account, reason) = (
            text(args, "line")?,
            text(args, "account")?,
            text(args, "reason")?,
        );
        let lines = self.bank_statement()?.lines;
        let Some(l) = lines.iter().find(|l| l.id == line) else {
            return Err(ToolError(format!("no bank line {line}")));
        };
        if l.status == "booked" {
            return Err(ToolError(format!("bank line {line} is booked already")));
        }
        if !self.accounts.contains_key(account) {
            return Err(ToolError(format!("no account {account}")));
        }
        let amount = l.amount.minor;
        let proposed = DomainEntry {
            date: l.date.clone(),
            memo: l.counterparty.clone(),
            // Money out is a cost (debit); money in is income (credit).
            lines: vec![
                DomainEntryLine {
                    account: account.to_owned(),
                    amount_minor: -amount,
                    vat_code: None,
                },
                DomainEntryLine {
                    account: self.domain.entity.bank_account.clone(),
                    amount_minor: amount,
                    vat_code: None,
                },
            ],
            settles: Vec::new(),
            reverse_charge: None,
            vat_split: None,
        };
        // The kernel's own checks (a leaf account, an open period), rolled back.
        let new = skyla_ledger::NewEntry {
            date: proposed.date.clone(),
            source_kind: skyla_ledger::SourceKind::Advisor,
            source_ref: None,
            memo: proposed.memo.clone(),
            created_by: "advisor".into(),
            lines: proposed
                .lines
                .iter()
                .map(|l| {
                    skyla_ledger::NewLine::debit(
                        &l.account,
                        skyla_money::Money::new(l.amount_minor, self.currency),
                    )
                })
                .collect(),
        };
        skyla_ledger::check_entry(&self.db(), &new)
            .map_err(|e| ToolError(format!("the kernel refused it: {e}")))?;
        let entry = self.proposed_entry(&proposed)?;
        self.file_entry(
            ProposalDto {
                id: String::new(),
                kind: "posting".into(),
                title: format!(
                    "Book “{}” to {account} {}",
                    l.counterparty,
                    self.account_name(account)
                ),
                detail: "Suggested by an advisor · for your review".into(),
                confidence: Some("needs_you".into()),
                source_kind: "advisor".into(),
                source: "Financial advisor".into(),
                bank_line_id: Some(line.to_owned()),
                due_on: None,
                amount: Some(l.amount.clone()),
                entry: Some(entry),
                reasons: vec![reason.to_owned()],
            },
            Some(proposed),
        )
    }

    fn propose_entry(&self, args: &Value) -> Result<Value, ToolError> {
        let date = text(args, "date")?;
        let memo = text(args, "memo")?;
        let reason = text(args, "reason")?;
        let lines: Vec<DomainEntryLine> = args
            .get("lines")
            .and_then(Value::as_array)
            .ok_or_else(|| ToolError("lines are required".into()))?
            .iter()
            .map(|l| {
                Ok(DomainEntryLine {
                    account: text(l, "account")?.to_owned(),
                    amount_minor: l
                        .get("amount_minor")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| ToolError("amount_minor must be an integer".into()))?,
                    vat_code: l.get("vat_code").and_then(Value::as_str).map(str::to_owned),
                })
            })
            .collect::<Result<_, ToolError>>()?;
        // The kernel's own checks, rolled back whatever happens.
        let new = skyla_ledger::NewEntry {
            date: date.to_owned(),
            source_kind: skyla_ledger::SourceKind::Advisor,
            source_ref: None,
            memo: memo.to_owned(),
            created_by: "advisor".into(),
            lines: lines
                .iter()
                .map(|l| skyla_ledger::NewLine {
                    vat_code: l.vat_code.clone(),
                    ..skyla_ledger::NewLine::debit(
                        &l.account,
                        skyla_money::Money::new(l.amount_minor, self.currency),
                    )
                })
                .collect(),
        };
        skyla_ledger::check_entry(&self.db(), &new)
            .map_err(|e| ToolError(format!("the kernel refused it: {e}")))?;
        let proposed = DomainEntry {
            date: date.to_owned(),
            memo: memo.to_owned(),
            lines,
            settles: Vec::new(),
            reverse_charge: None,
            vat_split: None,
        };
        // The kernel's own checks (a leaf account, an open period), rolled back.
        let new = skyla_ledger::NewEntry {
            date: proposed.date.clone(),
            source_kind: skyla_ledger::SourceKind::Advisor,
            source_ref: None,
            memo: proposed.memo.clone(),
            created_by: "advisor".into(),
            lines: proposed
                .lines
                .iter()
                .map(|l| {
                    skyla_ledger::NewLine::debit(
                        &l.account,
                        skyla_money::Money::new(l.amount_minor, self.currency),
                    )
                })
                .collect(),
        };
        skyla_ledger::check_entry(&self.db(), &new)
            .map_err(|e| ToolError(format!("the kernel refused it: {e}")))?;
        let entry = self.proposed_entry(&proposed)?;
        self.file_entry(
            ProposalDto {
                id: String::new(),
                kind: "posting".into(),
                title: memo.to_owned(),
                detail: "Proposed by an advisor · checked by the kernel · not posted".into(),
                confidence: Some("needs_you".into()),
                source_kind: "advisor".into(),
                source: "Advisor".into(),
                bank_line_id: None,
                due_on: None,
                amount: Some(entry.total_debit.clone()),
                entry: Some(entry),
                reasons: vec![reason.to_owned()],
            },
            Some(proposed),
        )
    }
}
