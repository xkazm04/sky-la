//! Inline "explain this" (WP-28, D-015): the core assembles what's behind a
//! figure (the entries, the previous period), the advisor explains it, and
//! the answer is shown only if every figure is grounded and every entry it
//! cites is one of the entries behind the figure.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};

use super::Core;
use crate::dto::{ExplainTargetDto, ExplainedEntryDto, ExplanationDto, MoneyDto};
use crate::error::CoreError;

/// The answer's shape.
pub fn explain_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "explanation": { "type": "string", "description": "Plain language. Quote figures exactly as given, in Czech format. Cite entries as #id." },
            "cites": { "type": "array", "items": { "type": "integer" }, "minItems": 1, "description": "The entry ids the explanation rests on." }
        },
        "required": ["explanation", "cites"],
        "additionalProperties": false
    })
}

const EXPLAIN_SYSTEM_PROMPT: &str = "You explain one figure, account or line in a Czech freelancer's books inside sky-la. \
Use only the context you're given: the figure, the previous period and the entries behind it. \
Never calculate a figure that isn't in the context; quote figures exactly, in Czech format (120 800 Kč). \
Cite every entry you rely on as #id and list their ids in cites. You never post anything; this is an explanation for the user.";

#[allow(clippy::expect_used)] // A constant pattern.
static CITED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#(\d+)").expect("citation pattern"));

impl Core {
    /// What's behind a target: the context the advisor gets, the entries it
    /// may cite, and the figures it may quote.
    pub fn explain_context(
        &self,
        target: &ExplainTargetDto,
    ) -> Result<(Value, Vec<ExplainedEntryDto>), CoreError> {
        let money = |m: i64| MoneyDto::try_from(skyla_money::Money::new(m, self.currency));
        match target.kind.as_str() {
            "account" => {
                let code = target.account.as_deref().ok_or_else(|| {
                    CoreError::BadRequest("an account target needs an account".into())
                })?;
                let (from, to) = (
                    target
                        .from
                        .as_deref()
                        .ok_or_else(|| CoreError::BadRequest("from is required".into()))?,
                    target
                        .to
                        .as_deref()
                        .ok_or_else(|| CoreError::BadRequest("to is required".into()))?,
                );
                if !self.accounts.contains_key(code) {
                    return Err(CoreError::BadRequest(format!("no account {code}")));
                }
                // The previous period of the same length, ending the day before.
                let (f, t) = (
                    skyla_rules::date::parse(from)
                        .ok_or_else(|| CoreError::BadRequest(format!("{from} isn't a date")))?,
                    skyla_rules::date::parse(to)
                        .ok_or_else(|| CoreError::BadRequest(format!("{to} isn't a date")))?,
                );
                let (pf, pt) = (
                    skyla_rules::date::format(f - (t - f) - 1),
                    skyla_rules::date::format(f - 1),
                );
                let collect =
                    |a: &str, z: &str| -> Result<(i64, Vec<ExplainedEntryDto>), CoreError> {
                        let mut total = 0;
                        let mut out = Vec::new();
                        for e in skyla_ledger::list_posted(&self.db(), a, z)? {
                            let amount: i64 = e
                                .lines
                                .iter()
                                .filter(|l| l.account == code)
                                .map(|l| l.functional.minor())
                                .sum();
                            if e.lines.iter().any(|l| l.account == code) {
                                total += amount;
                                out.push(ExplainedEntryDto {
                                    id: e.id,
                                    date: e.date,
                                    memo: e.memo,
                                    amount: money(amount)?,
                                });
                            }
                        }
                        Ok((total, out))
                    };
                let (total, entries) = collect(from, to)?;
                let (previous, _) = collect(&pf, &pt)?;
                let change = total - previous;
                let percent = if previous == 0 {
                    None
                } else {
                    let h = change * 10_000 / previous;
                    Some((h + 50 * h.signum()) / 100)
                };
                let context = json!({
                    "account": code,
                    "name": self.account_name(code),
                    "period": { "from": from, "to": to, "total": money(total)? },
                    "previous": { "from": pf, "to": pt, "total": money(previous)? },
                    "change": money(change)?,
                    "changePercent": percent.map(|p| p.to_string()),
                    "entries": entries.iter().map(|e| json!({ "id": e.id, "date": e.date, "memo": e.memo, "amount": e.amount })).collect::<Vec<_>>(),
                });
                Ok((context, entries))
            }
            "entry" => {
                let id = target.entry.ok_or_else(|| {
                    CoreError::BadRequest("an entry target needs an entry".into())
                })?;
                let e = skyla_ledger::get_entry(&self.db(), id)?;
                let lines: Vec<Value> = e
                    .lines
                    .iter()
                    .map(|l| Ok(json!({ "account": l.account, "name": self.account_name(&l.account), "amount": money(l.functional.minor())?, "vatCode": l.vat_code })))
                    .collect::<Result<_, CoreError>>()?;
                let gross: i64 = e
                    .lines
                    .iter()
                    .map(|l| l.functional.minor())
                    .filter(|m| *m > 0)
                    .sum();
                let entry = ExplainedEntryDto {
                    id: e.id,
                    date: e.date.clone(),
                    memo: e.memo.clone(),
                    amount: money(gross)?,
                };
                Ok((
                    json!({ "entry": { "id": e.id, "date": e.date, "memo": e.memo, "lines": lines } }),
                    vec![entry],
                ))
            }
            other => Err(CoreError::BadRequest(format!("can't explain a {other}"))),
        }
    }

    /// Explains a target with the advisor.
    pub fn explain(
        &self,
        target: &ExplainTargetDto,
        confirmed: bool,
    ) -> Result<ExplanationDto, CoreError> {
        use skyla_advisor::grounding::{Allowed, check};
        use skyla_advisor::{RunRequest, RunStatus};
        use skyla_egress::Policy;

        let task = crate::core::egress::task("explain.figure")?;
        let (context, entries) = self.explain_context(target)?;
        let mut out = ExplanationDto {
            status: String::new(),
            text: None,
            cites: Vec::new(),
            problems: Vec::new(),
            grounded: 0,
            run_id: None,
            entries: entries.clone(),
        };
        match self.egress_policy(task.id) {
            Policy::Never => {
                out.status = "blocked".into();
                out.problems
                    .push("Settings say explanations never run.".into());
                return Ok(out);
            }
            Policy::Ask if !confirmed => {
                out.status = "needs_confirmation".into();
                return Ok(out);
            }
            _ => {}
        }
        let gate = self.gate_for(task.id)?;
        let (gated_context, mut report) = gate.json(&context);
        let what = match target.kind.as_str() {
            "account" => format!(
                "Explain account {} for {} to {} against the previous period.",
                target.account.as_deref().unwrap_or(""),
                target.from.as_deref().unwrap_or(""),
                target.to.as_deref().unwrap_or("")
            ),
            _ => format!("Explain entry #{}.", target.entry.unwrap_or_default()),
        };
        let prompt = format!("{what}\nContext:\n{gated_context}");
        let (prompt, r) = gate.text(&prompt);
        report.merge(r);
        let shim = self.shim.lock().map(|s| s.clone()).unwrap_or_default();
        let (outcome, calls) = self.with_tool_host(&gate, |run| {
            let request = RunRequest {
                task: task.id.into(),
                system_prompt: EXPLAIN_SYSTEM_PROMPT.into(),
                prompt: prompt.clone(),
                schema: Some(explain_schema()),
                mcp_config: Some(run.mcp_config(&shim)),
                server: "skyla".into(),
                model: None,
                effort: Some("low".into()),
                max_turns: 4,
            };
            match self.provider.lock() {
                Ok(p) => p.run(&request, &mut |_| {}),
                Err(_) => skyla_advisor::RunOutcome::with_status(RunStatus::Failed(
                    "the provider is unavailable".into(),
                )),
            }
        })?;
        let mut allowed = Allowed::new();
        allowed_from(&context, &mut allowed);
        let ids: BTreeSet<i64> = entries.iter().map(|e| e.id).collect();
        let note;
        if outcome.status == RunStatus::Completed {
            let answer = outcome.structured.clone().unwrap_or_default();
            let text = answer["explanation"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let cites: Vec<i64> = answer["cites"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            if text.is_empty() || cites.is_empty() {
                out.problems
                    .push("The answer must explain and cite at least one entry.".into());
            }
            let mentioned: BTreeSet<i64> = CITED
                .captures_iter(&text)
                .filter_map(|c| c[1].parse().ok())
                .collect();
            for id in cites.iter().chain(&mentioned) {
                if !ids.contains(id) {
                    out.problems
                        .push(format!("It cites #{id}, which isn't behind this figure."));
                }
            }
            // Entry ids aren't figures.
            let without_ids = CITED.replace_all(&text, "");
            let g = check(&without_ids, &allowed, 13);
            out.grounded = u32::try_from(g.grounded).unwrap_or(u32::MAX);
            for u in g.ungrounded {
                out.problems.push(format!(
                    "{} (in “{}”) isn't a figure from the books.",
                    u.text, u.context
                ));
            }
            out.problems.dedup();
            if out.problems.is_empty() {
                out.status = "accepted".into();
                out.text = Some(gate.pseudonyms().reveal(&text));
                let mut c = cites;
                c.sort_unstable();
                c.dedup();
                out.cites = c;
                note = format!("Explained; cites {} entries", out.cites.len());
            } else {
                out.status = "rejected".into();
                note = format!("Rejected: {} problem(s); nothing shown", out.problems.len());
            }
        } else {
            out.status = "failed".into();
            out.problems.push(format!("{:?}", outcome.status));
            note = "Not completed".into();
        }
        for c in &calls {
            report.merge(c.report.clone());
        }
        let results: Vec<(String, Value, String)> = calls
            .iter()
            .map(|c| (c.name.clone(), c.arguments.clone(), c.result.clone()))
            .collect();
        let n = self.egress_register()?.len() + 1;
        let uid = format!("run-{}-{n:02}", self.domain.entity.as_of);
        let provider = self
            .provider
            .lock()
            .map(|p| p.id().to_owned())
            .unwrap_or_default();
        let purpose = match target.kind.as_str() {
            "account" => format!(
                "Explain this · account {}",
                target.account.as_deref().unwrap_or("")
            ),
            _ => format!("Explain this · entry #{}", target.entry.unwrap_or_default()),
        };
        self.record_run(
            &uid,
            &format!("{}T12:00:00Z", self.domain.entity.as_of),
            task,
            &purpose,
            &provider,
            outcome.model.as_deref().unwrap_or("your default"),
            &report,
            u32::try_from(calls.len()).unwrap_or(u32::MAX),
            crate::core::egress::payload_bytes(task.id, &prompt, &results),
            &note,
            outcome
                .cost_estimate_usd
                .map(|c| format!("about ${c:.4} (the provider's estimate)")),
        )?;
        out.run_id = Some(uid);
        Ok(out)
    }
}

/// Amounts and percentages in the context.
fn allowed_from(v: &Value, into: &mut skyla_advisor::grounding::Allowed) {
    match v {
        Value::Object(m) => {
            if let (Some(minor), Some(_)) =
                (m.get("minor").and_then(Value::as_i64), m.get("currency"))
            {
                into.amount(minor);
            }
            if let Some(p) = m.get("changePercent").and_then(Value::as_str) {
                into.percent(p);
            }
            m.values().for_each(|x| allowed_from(x, into));
        }
        Value::Array(a) => a.iter().for_each(|x| allowed_from(x, into)),
        _ => {}
    }
}
