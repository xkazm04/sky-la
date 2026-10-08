//! The advisor provider: which one the core runs on and whether it can run
//! (WP-24). Runs themselves arrive with the advisors (WP-27, WP-28).

use skyla_advisor::{Availability, LlmProvider};

use super::Core;
use crate::dto::AdvisorStatusDto;
use crate::error::CoreError;

/// `1791763200` → `2026-10-08T00:00:00Z`; anything else passes through.
fn rfc3339(unix: &str) -> String {
    let Ok(secs) = unix.parse::<i64>() else {
        return unix.to_owned();
    };
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    format!(
        "{}T{:02}:{:02}:{:02}Z",
        skyla_rules::date::format(days),
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

impl Core {
    /// Swaps the provider (the desktop shell's CLI driver, or a test's fake).
    pub fn replace_provider(&self, provider: Box<dyn LlmProvider>) {
        if let Ok(mut p) = self.provider.lock() {
            *p = provider;
        }
    }

    /// Whether advisors can run now.
    pub fn advisor_status(&self) -> AdvisorStatusDto {
        let (id, availability) = match self.provider.lock() {
            Ok(p) => (p.id().to_owned(), p.availability()),
            Err(_) => (
                "unknown".to_owned(),
                Availability::NotInstalled {
                    looked_in: Vec::new(),
                },
            ),
        };
        let mut dto = AdvisorStatusDto {
            demo: id == "fake",
            provider: id,
            state: String::new(),
            version: None,
            looked_in: Vec::new(),
            resets_at: None,
        };
        match availability {
            Availability::Ready { version } => {
                dto.state = "ready".into();
                dto.version = Some(version);
            }
            Availability::NotInstalled { looked_in } => {
                dto.state = "not_installed".into();
                dto.looked_in = looked_in;
            }
            Availability::NotSignedIn => dto.state = "not_signed_in".into(),
            Availability::RateLimited { resets_at } => {
                dto.state = "rate_limited".into();
                dto.resets_at = resets_at.as_deref().map(rfc3339);
            }
        }
        dto
    }
}

/// The answer the tax advisor must return (`--json-schema`).
pub fn tax_answer_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "recommendation": { "type": "string", "description": "The id of the scenario you'd put first for the user's review." },
            "explanation": { "type": "string", "description": "Plain-language explanation. Quote figures exactly as the tools return them, in Czech format (942 600 Kč)." },
            "levers": { "type": "array", "items": { "type": "string" } },
            "questions": { "type": "array", "items": { "type": "string" }, "description": "Facts the books can't hold that would change the answer." }
        },
        "required": ["recommendation", "explanation", "levers", "questions"],
        "additionalProperties": false
    })
}

const TAX_SYSTEM_PROMPT: &str = "You are the tax advisor inside sky-la, a bookkeeping app for Czech freelancers (OSVČ). \
You explain scenarios; you never post anything and you never give tax advice: everything you write is a scenario for the user's review. \
Every figure must come from a tool: call run_scenario for the scenarios and get_rule_value for any statutory value. \
Never calculate, round or estimate a figure yourself, and quote figures exactly as the tools return them, in Czech format (942 600 Kč, 29,2 %). \
Name the levers you considered, put one scenario first by its id, say what each choice changes (tax, insurance, the pension assessment base), \
and list facts the books can't hold that would change the answer.";

/// Pack values a tax explanation may quote besides the scenarios' own.
const TAX_VALUES: &[&str] = &[
    "income_tax.rate.base",
    "income_tax.credit.taxpayer",
    "income_tax.flat_rate.craft.percent",
    "income_tax.flat_rate.craft.cap",
    "income_tax.flat_rate.trade.percent",
    "income_tax.flat_rate.trade.cap",
    "income_tax.flat_rate.liberal.percent",
    "income_tax.flat_rate.liberal.cap",
    "assets.tangible.threshold",
    "insurance.social.share",
    "insurance.social.rate",
    "insurance.health.share",
    "insurance.health.rate",
];

/// Every amount (`{ minor, currency }`) and percentage in a JSON value.
fn collect_allowed(v: &serde_json::Value, into: &mut skyla_advisor::grounding::Allowed) {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            if let (Some(m), Some(_)) = (
                map.get("minor").and_then(Value::as_i64),
                map.get("currency"),
            ) {
                into.amount(m);
            }
            for (k, x) in map {
                if k.to_lowercase().ends_with("percent")
                    && let Some(t) = x.as_str()
                {
                    into.percent(&t.replace(',', "."));
                }
                collect_allowed(x, into);
            }
        }
        Value::Array(items) => items.iter().for_each(|x| collect_allowed(x, into)),
        _ => {}
    }
}

impl Core {
    /// Where the `skyla-mcp` shim is (next to the app binary in a build).
    pub fn set_shim_path(&self, path: std::path::PathBuf) {
        if let Ok(mut s) = self.shim.lock() {
            *s = path;
        }
    }

    /// "Now" for the register: the demo's fixed clock, so recordings are
    /// reproducible; the real store (WP-30) uses the system clock.
    fn run_time(&self) -> String {
        format!("{}T12:00:00Z", self.domain.entity.as_of)
    }

    /// Asks the tax advisor to explain the scenarios. Every figure in its
    /// answer must be one the engine produced, or the answer is rejected.
    pub fn run_tax_advisor(
        &self,
        projection: Option<&crate::dto::TaxProjectionDto>,
        confirmed: bool,
    ) -> Result<crate::dto::TaxAdviceDto, CoreError> {
        use skyla_advisor::grounding::{Allowed, check};
        use skyla_advisor::{RunRequest, RunStatus};
        use skyla_egress::Policy;

        let task = crate::core::egress::task("tax.scenarios")?;
        let scenarios = self.income_tax_scenarios(projection)?;
        let mut advice = crate::dto::TaxAdviceDto {
            status: String::new(),
            recommendation: None,
            explanation: None,
            levers: Vec::new(),
            questions: Vec::new(),
            problems: Vec::new(),
            grounded: 0,
            run_id: None,
            scenarios: scenarios.clone(),
        };
        match self.egress_policy(task.id) {
            Policy::Never => {
                advice.status = "blocked".into();
                advice
                    .problems
                    .push("Settings say the tax advisor never runs.".into());
                return Ok(advice);
            }
            Policy::Ask if !confirmed => {
                advice.status = "needs_confirmation".into();
                return Ok(advice);
            }
            _ => {}
        }
        let projection_json = serde_json::to_value(projection).unwrap_or(serde_json::Value::Null);
        let prompt = match projection {
            None => format!(
                "Tax year {}. Explain the § 7 scenarios from the books so far ({} to {}). Call run_scenario with projection null.",
                scenarios.year, scenarios.from, scenarios.to
            ),
            Some(p) => format!(
                "Tax year {}. The user projects § 7 income {} and actual expenses {}, flat-rate group {}, planned purchase {} {}. \
                 Call run_scenario with projection {} and explain the scenarios.",
                scenarios.year,
                p.income,
                p.expenses,
                p.flat_rate.as_deref().unwrap_or("unknown"),
                p.purchase_description,
                p.purchase_price,
                projection_json
            ),
        };
        let gate = self.gate_for(task.id)?;
        let (prompt, mut report) = gate.text(&prompt);
        let shim = self.shim.lock().map(|s| s.clone()).unwrap_or_default();
        let (outcome, calls) = self.with_tool_host(&gate, |run| {
            let request = RunRequest {
                task: task.id.into(),
                system_prompt: TAX_SYSTEM_PROMPT.into(),
                prompt: prompt.clone(),
                schema: Some(tax_answer_schema()),
                mcp_config: Some(run.mcp_config(&shim)),
                server: "skyla".into(),
                model: None,
                effort: Some("medium".into()),
                max_turns: 8,
            };
            match self.provider.lock() {
                Ok(p) => p.run(&request, &mut |_| {}),
                Err(_) => skyla_advisor::RunOutcome::with_status(RunStatus::Failed(
                    "the provider is unavailable".into(),
                )),
            }
        })?;
        for c in &calls {
            report.merge(c.report.clone());
        }

        // Allowed figures: the engine's own analysis, and what tools returned.
        let mut allowed = Allowed::new();
        collect_allowed(
            &serde_json::to_value(&scenarios).unwrap_or_default(),
            &mut allowed,
        );
        // The statutory values the tax advisor may quote, from the pack.
        let on = format!("{}-12-31", scenarios.year);
        for key in TAX_VALUES {
            match self.pack.value(key, &on).map(|v| v.kind) {
                Ok(skyla_rules::Kind::Percent) => {
                    if let Ok(p) = self.pack.percent(key, &on) {
                        allowed.percent(&p.normalize().to_string());
                    }
                }
                Ok(skyla_rules::Kind::Amount) => {
                    if let Ok(a) = self.pack.amount(key, &on) {
                        allowed.amount(a.minor());
                    }
                }
                _ => {}
            }
        }
        for c in &calls {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&c.result) {
                collect_allowed(&v, &mut allowed);
            }
        }

        let outcome_note: String;
        match &outcome.status {
            RunStatus::Completed => {
                let answer = outcome.structured.clone().unwrap_or_default();
                let recommendation = answer["recommendation"].as_str().map(str::to_owned);
                let explanation = answer["explanation"].as_str().map(str::to_owned);
                let strings = |k: &str| -> Vec<String> {
                    answer[k]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default()
                };
                advice.levers = strings("levers");
                advice.questions = strings("questions");
                match (&recommendation, &explanation) {
                    (Some(r), Some(e)) => {
                        if !scenarios.scenarios.iter().any(|s| &s.id == r) {
                            advice.problems.push(format!(
                                "It recommended {r:?}, which isn't one of the engine's scenarios."
                            ));
                        }
                        let mut text = e.clone();
                        for q in &advice.questions {
                            text.push('\n');
                            text.push_str(q);
                        }
                        let g = check(&text, &allowed, 13);
                        advice.grounded = u32::try_from(g.grounded).unwrap_or(u32::MAX);
                        for u in g.ungrounded {
                            advice.problems.push(format!(
                                "{} (in “{}”) isn't a figure the engine produced.",
                                u.text, u.context
                            ));
                        }
                    }
                    _ => advice
                        .problems
                        .push("The answer didn't follow the required shape.".into()),
                }
                if advice.problems.is_empty() {
                    advice.status = "accepted".into();
                    advice.recommendation = recommendation.clone();
                    advice.explanation = explanation.map(|e| gate.pseudonyms().reveal(&e));
                    let label = scenarios
                        .scenarios
                        .iter()
                        .find(|s| Some(&s.id) == recommendation.as_ref())
                        .map_or_else(String::new, |s| s.label.clone());
                    let mut reasons = vec![advice.explanation.clone().unwrap_or_default()];
                    reasons.extend(advice.questions.iter().map(|q| format!("Question: {q}")));
                    if let Ok(mut inbox) = self.advisor_inbox.lock() {
                        let proposal = crate::dto::ProposalDto {
                            id: String::new(),
                            kind: "advice".into(),
                            title: format!("Tax scenario for {}: {label}", scenarios.year),
                            detail: "Tax advisor · a scenario for your review, not tax advice"
                                .into(),
                            confidence: None,
                            source_kind: "advisor".into(),
                            source: "Tax advisor".into(),
                            bank_line_id: None,
                            due_on: None,
                            amount: None,
                            entry: None,
                            reasons,
                        };
                        inbox.file(proposal, None);
                    }
                    outcome_note = "Scenario explained; figures checked against the engine".into();
                } else {
                    advice.status = "rejected".into();
                    outcome_note = format!(
                        "Rejected: {} figure problem(s); nothing shown",
                        advice.problems.len()
                    );
                }
            }
            other => {
                advice.status = match other {
                    RunStatus::NotInstalled => "not_installed",
                    RunStatus::NotSignedIn => "not_signed_in",
                    RunStatus::RateLimited(_) => "rate_limited",
                    RunStatus::ProfileViolation(_) => "profile_violation",
                    RunStatus::Interrupted => "interrupted",
                    _ => "failed",
                }
                .into();
                advice.problems.push(match other {
                    RunStatus::Failed(m) => m.clone(),
                    RunStatus::ProfileViolation(v) => {
                        format!("Stopped by the launch profile: {}", v.join("; "))
                    }
                    _ => format!("{other:?}"),
                });
                outcome_note = format!("Not completed: {}", advice.status.replace('_', " "));
            }
        }

        // Every run is recorded, accepted or not.
        let results: Vec<(String, serde_json::Value, String)> = calls
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
        self.record_run(
            &uid,
            &self.run_time(),
            task,
            "Explain the tax scenarios",
            &provider,
            outcome.model.as_deref().unwrap_or("your default"),
            &report,
            u32::try_from(calls.len()).unwrap_or(u32::MAX),
            crate::core::egress::payload_bytes(task.id, &prompt, &results),
            &outcome_note,
            outcome
                .cost_estimate_usd
                .map(|c| format!("about ${c:.4} (the provider's estimate)")),
        )?;
        advice.run_id = Some(uid);
        Ok(advice)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unix_seconds_become_utc() {
        assert_eq!(super::rfc3339("1791763200"), "2026-10-12T00:00:00Z");
        assert_eq!(super::rfc3339("1791806400"), "2026-10-12T12:00:00Z");
        assert_eq!(super::rfc3339("soon"), "soon");
    }
}
