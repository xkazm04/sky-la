//! The egress side of advisor runs (WP-26): which task may send what, the
//! gate built for each run, and the register in the entity's database.
//!
//! Every tool result goes through [`Core::gated_tool_call`] before it
//! leaves; every run is appended to the register with the exact bytes sent.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use skyla_egress::register::{self, NewRun};
use skyla_egress::{FieldClass, Gate, GateReport, Policy, Pseudonyms, Role};

use super::Core;
use crate::dto::{EgressPayloadDto, EgressPolicyDto, EgressRunDto};
use crate::error::CoreError;

/// An advisor task type and what it may send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvisorTask {
    /// Stable id, e.g. `tax.scenarios`.
    pub id: &'static str,
    /// Which advisor runs it.
    pub advisor: &'static str,
    /// For people.
    pub label: &'static str,
    /// The field classes it may send.
    pub scope: &'static [FieldClass],
}

/// Every task type, with its scope. Names stay pseudonymised everywhere:
/// no task needs them to reason about the books.
pub const TASKS: &[AdvisorTask] = &[
    AdvisorTask {
        id: "tax.scenarios",
        advisor: "Tax advisor",
        label: "Tax scenarios and explanations",
        scope: &[FieldClass::Aggregates, FieldClass::AccountTotals],
    },
    AdvisorTask {
        id: "financial.findings",
        advisor: "Financial advisor",
        label: "Findings about costs, margins and trends",
        scope: &[FieldClass::Aggregates, FieldClass::AccountTotals],
    },
    AdvisorTask {
        id: "explain.figure",
        advisor: "Financial advisor",
        label: "Explain this (a figure, account or line)",
        scope: &[
            FieldClass::Aggregates,
            FieldClass::AccountTotals,
            FieldClass::LineMemos,
        ],
    },
    AdvisorTask {
        id: "bank.categorise",
        advisor: "Advisor suggestion",
        label: "Suggest an account for a bank line",
        scope: &[FieldClass::Aggregates, FieldClass::LineMemos],
    },
];

fn class_name(c: FieldClass) -> &'static str {
    match c {
        FieldClass::Aggregates => "aggregates",
        FieldClass::AccountTotals => "account_totals",
        FieldClass::CounterpartyNames => "counterparty_names",
        FieldClass::LineMemos => "line_memos",
        FieldClass::Documents => "documents",
    }
}

fn class_label(c: FieldClass) -> &'static str {
    match c {
        FieldClass::Aggregates => "Totals for periods",
        FieldClass::AccountTotals => "Account balances and movements",
        FieldClass::CounterpartyNames => "Customer and supplier names",
        FieldClass::LineMemos => "Line memos and payment messages",
        FieldClass::Documents => "Whole documents",
    }
}

fn policy_name(p: Policy) -> &'static str {
    match p {
        Policy::Always => "always",
        Policy::Ask => "ask",
        Policy::Never => "never",
    }
}

/// The task, or a refusal naming it.
pub fn task(id: &str) -> Result<&'static AdvisorTask, CoreError> {
    TASKS
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| CoreError::BadRequest(format!("no advisor task {id}")))
}

/// The payload a run sent, exactly as it's stored and replayed.
pub fn payload_bytes(task: &str, prompt: &str, results: &[(String, Value, String)]) -> Vec<u8> {
    let tool_results: Vec<Value> = results
        .iter()
        .map(|(tool, arguments, result)| json!({ "tool": tool, "arguments": arguments, "result": result }))
        .collect();
    json!({ "task": task, "prompt": prompt, "toolResults": tool_results })
        .to_string()
        .into_bytes()
}

impl Core {
    /// Pseudonyms for everyone the books name: customers from invoicing,
    /// suppliers from purchases and the bank.
    pub(crate) fn pseudonyms(&self) -> Pseudonyms {
        let mut parties: Vec<(String, Role)> = Vec::new();
        for c in &self.domain.clients {
            parties.push((c.name.clone(), Role::Customer));
            parties.push((c.legal_name.clone(), Role::Customer));
        }
        for p in &self.domain.purchases {
            parties.push((p.supplier.clone(), Role::Vendor));
        }
        if let Ok(bank) = self.bank_statement() {
            for l in bank.lines {
                let role = if l.amount.minor >= 0 {
                    Role::Customer
                } else {
                    Role::Vendor
                };
                parties.push((l.counterparty, role));
            }
        }
        Pseudonyms::new(&parties)
    }

    /// The gate for one run of `task`.
    pub fn gate_for(&self, task_id: &str) -> Result<Gate, CoreError> {
        let t = task(task_id)?;
        Ok(Gate::new(t.scope.iter().copied(), self.pseudonyms()))
    }

    /// Calls a tool and gates its result: what this returns is exactly what
    /// may leave the machine.
    pub fn gated_tool_call(
        &self,
        gate: &Gate,
        name: &str,
        args: &Value,
    ) -> (String, GateReport, bool) {
        let mut report = GateReport::default();
        match self.call_tool(name, args) {
            Ok(v) => {
                let (out, r) = gate.json(&v);
                report.merge(r);
                (out.to_string(), report, false)
            }
            Err(e) => {
                let (out, r) = gate.text(&e.to_string());
                report.merge(r);
                (out, report, true)
            }
        }
    }

    /// Appends a run to the register.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_run(
        &self,
        uid: &str,
        at: &str,
        task: &AdvisorTask,
        purpose: &str,
        provider: &str,
        model: &str,
        withheld: &GateReport,
        tool_calls: u32,
        payload: Vec<u8>,
        outcome: &str,
        cost_note: Option<String>,
    ) -> Result<i64, CoreError> {
        register::record(
            &self.db(),
            &NewRun {
                uid: uid.to_owned(),
                at: at.to_owned(),
                task: task.id.to_owned(),
                advisor: task.advisor.to_owned(),
                purpose: purpose.to_owned(),
                provider: provider.to_owned(),
                model: model.to_owned(),
                scopes: task
                    .scope
                    .iter()
                    .map(|c| class_name(*c).to_owned())
                    .collect(),
                withheld: withheld.summary(),
                tool_calls,
                payload,
                outcome: outcome.to_owned(),
                cost_note,
            },
        )
        .map_err(|e| CoreError::BadRequest(e.to_string()))
    }

    /// The register, newest first.
    pub fn egress_register(&self) -> Result<Vec<EgressRunDto>, CoreError> {
        let db = self.db();
        let runs = register::list(&db).map_err(|e| CoreError::BadRequest(e.to_string()))?;
        let broken = register::verify(&db).map_err(|e| CoreError::BadRequest(e.to_string()))?;
        runs.into_iter()
            .map(|r| {
                let bytes = register::replay(&db, r.id)
                    .map_err(|e| CoreError::BadRequest(e.to_string()))?;
                let scopes: Vec<String> = r
                    .run
                    .scopes
                    .iter()
                    .filter_map(|s| {
                        TASKS
                            .iter()
                            .flat_map(|t| t.scope)
                            .find(|c| class_name(**c) == s)
                            .map(|c| class_label(*c).to_owned())
                    })
                    .collect();
                Ok(EgressRunDto {
                    id: r.run.uid,
                    at: r.run.at,
                    advisor: r.run.advisor,
                    purpose: r.run.purpose,
                    provider: r.run.provider,
                    model: r.run.model,
                    sent: if scopes.is_empty() {
                        "Nothing beyond the prompt".into()
                    } else {
                        scopes.join("; ")
                    },
                    redacted: r.run.withheld,
                    tool_calls: r.run.tool_calls,
                    bytes_sent: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
                    outcome: r.run.outcome,
                    cost_note: r.run.cost_note,
                    intact: broken.is_none_or(|b| r.id < b),
                })
            })
            .collect()
    }

    /// Exactly what a run sent ("What was shared").
    pub fn egress_payload(&self, id: &str) -> Result<EgressPayloadDto, CoreError> {
        let db = self.db();
        let runs = register::list(&db).map_err(|e| CoreError::BadRequest(e.to_string()))?;
        let run = runs
            .into_iter()
            .find(|r| r.run.uid == id)
            .ok_or_else(|| CoreError::BadRequest(format!("no run {id} in the register")))?;
        let bytes =
            register::replay(&db, run.id).map_err(|e| CoreError::BadRequest(e.to_string()))?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        // For reading only: tool results were sent as JSON text, so show them
        // decoded. The stored and replayed bytes stay exactly as sent.
        let pretty = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .map(|mut v| {
                if let Some(results) = v.get_mut("toolResults").and_then(Value::as_array_mut) {
                    for r in results {
                        if let Some(decoded) = r["result"]
                            .as_str()
                            .and_then(|t| serde_json::from_str::<Value>(t).ok())
                        {
                            r["result"] = decoded;
                        }
                    }
                }
                v
            })
            .and_then(|v| serde_json::to_string_pretty(&v).ok())
            .unwrap_or_else(|| text.clone());
        Ok(EgressPayloadDto {
            id: run.run.uid,
            bytes: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            hash: run.hash,
            text: pretty,
        })
    }

    /// Each task type with its scope and the user's policy.
    pub fn egress_policies(&self) -> Vec<EgressPolicyDto> {
        let policies = self
            .egress_policies
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default();
        TASKS
            .iter()
            .map(|t| EgressPolicyDto {
                task: t.id.to_owned(),
                advisor: t.advisor.to_owned(),
                label: t.label.to_owned(),
                scope: t.scope.iter().map(|c| class_label(*c).to_owned()).collect(),
                policy: policy_name(policies.get(t.id).copied().unwrap_or(Policy::Ask)).to_owned(),
            })
            .collect()
    }

    /// Sets a task's policy (`always`, `ask` or `never`).
    pub fn set_egress_policy(
        &self,
        task_id: &str,
        policy: &str,
    ) -> Result<Vec<EgressPolicyDto>, CoreError> {
        let t = task(task_id)?;
        let p = match policy {
            "always" => Policy::Always,
            "ask" => Policy::Ask,
            "never" => Policy::Never,
            other => {
                return Err(CoreError::BadRequest(format!(
                    "policy {other:?} isn't always, ask or never"
                )));
            }
        };
        if let Ok(mut map) = self.egress_policies.lock() {
            map.insert(t.id, p);
        }
        Ok(self.egress_policies())
    }

    /// The policy for a task.
    pub fn egress_policy(&self, task_id: &str) -> Policy {
        self.egress_policies
            .lock()
            .ok()
            .and_then(|p| p.get(task_id).copied())
            .unwrap_or(Policy::Ask)
    }

    /// The demo's past runs, replayed through the real gate at opening so
    /// the register shows what the gate does with this data.
    pub(crate) fn seed_demo_register(&self) -> Result<(), CoreError> {
        register::apply_schema(&self.db()).map_err(|e| CoreError::BadRequest(e.to_string()))?;
        let mut runs = self.domain.egress_runs.clone();
        runs.sort_by(|a, b| a.at.cmp(&b.at));
        for r in runs {
            let t = task(&r.task)?;
            let gate = self.gate_for(t.id)?;
            let (prompt, mut report) = gate.text(&r.prompt);
            let mut results = Vec::new();
            for call in &r.tool_calls {
                let (out, rep, _) = self.gated_tool_call(&gate, &call.name, &call.arguments);
                report.merge(rep);
                results.push((call.name.clone(), gate.json(&call.arguments).0, out));
            }
            let payload = payload_bytes(t.id, &prompt, &results);
            self.record_run(
                &r.id,
                &r.at,
                t,
                &r.purpose,
                "Claude Code (your installation)",
                "your default",
                &report,
                u32::try_from(results.len()).unwrap_or(u32::MAX),
                payload,
                &r.outcome,
                None,
            )?;
        }
        Ok(())
    }
}

/// Policies the user set, by task.
pub(crate) type Policies = BTreeMap<&'static str, Policy>;
