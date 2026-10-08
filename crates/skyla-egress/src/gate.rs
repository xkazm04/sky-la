//! The gate every prompt and tool result passes before it reaches a model:
//! identifiers are always withheld, counterparty names become stable
//! pseudonyms unless the task's scope grants names, and fields outside the
//! scope are dropped.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::redact::{Withheld, redact};

/// What a task may send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldClass {
    /// Totals and figures for periods.
    Aggregates,
    /// Balances and movements per account.
    AccountTotals,
    /// Names of customers and suppliers.
    CounterpartyNames,
    /// Free text on lines: memos, payment messages, references.
    LineMemos,
    /// Whole documents (invoices, statements).
    Documents,
}

/// Whether a task may run, per task type, as the user set it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    /// Runs without asking.
    Always,
    /// Asks before each run.
    Ask,
    /// Never runs.
    Never,
}

/// A party the books know, for pseudonyms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Role {
    /// Someone who pays us.
    Customer,
    /// Someone we pay.
    Vendor,
}

/// Names and the stable pseudonyms that stand for them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pseudonyms {
    /// Real name (and its short form) → pseudonym.
    by_name: BTreeMap<String, String>,
    /// Pseudonym → the full name, to put back.
    reveal: BTreeMap<String, String>,
}

/// Legal forms a name may carry or drop ("Pixelfarm s.r.o." is "Pixelfarm").
const LEGAL_FORMS: &[&str] = &[
    "spol. s r.o.",
    "s. r. o.",
    "s.r.o.",
    "a. s.",
    "a.s.",
    "v.o.s.",
    "k.s.",
    "z.s.",
    "SE",
    "SARL",
    "GmbH",
    "Ltd",
    "Inc.",
    "LLC",
];

/// The name without a trailing legal form.
fn short_form(name: &str) -> String {
    let mut n = name.trim().trim_end_matches(',').to_owned();
    for form in LEGAL_FORMS {
        if let Some(rest) = n.strip_suffix(form) {
            n = rest.trim().trim_end_matches(',').trim().to_owned();
            break;
        }
    }
    n
}

fn letters(mut n: usize) -> String {
    // A, B, …, Z, AA, AB, …
    let mut s = String::new();
    loop {
        s.insert(0, char::from(b'A' + u8::try_from(n % 26).unwrap_or(0)));
        if n < 26 {
            return s;
        }
        n = n / 26 - 1;
    }
}

impl Pseudonyms {
    /// Pseudonyms for `parties`, assigned in name order per role, so the
    /// same books always give the same letters.
    pub fn new(parties: &[(String, Role)]) -> Self {
        // One pseudonym per party, however its name is written.
        let mut full: BTreeMap<(Role, String), String> = BTreeMap::new();
        for (name, role) in parties {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let entry = full
                .entry((*role, short_form(name)))
                .or_insert_with(|| name.to_owned());
            if name.len() > entry.len() {
                *entry = name.to_owned();
            }
        }
        let mut by_name = BTreeMap::new();
        let mut reveal = BTreeMap::new();
        let (mut c, mut v) = (0, 0);
        for ((role, short), name) in full {
            if by_name.contains_key(&name) {
                continue;
            }
            let label = match role {
                Role::Customer => {
                    c += 1;
                    format!("Customer {}", letters(c - 1))
                }
                Role::Vendor => {
                    v += 1;
                    format!("Vendor {}", letters(v - 1))
                }
            };
            if short.chars().count() >= 3 {
                by_name.insert(short, label.clone());
            }
            by_name.insert(name.clone(), label.clone());
            reveal.insert(label, name);
        }
        Self { by_name, reveal }
    }

    /// Replaces every known name (longest first) with its pseudonym.
    pub fn hide(&self, text: &str) -> (String, usize) {
        let mut names: Vec<(&String, &String)> = self.by_name.iter().collect();
        names.sort_by_key(|(n, _)| std::cmp::Reverse(n.len()));
        let mut out = text.to_owned();
        let mut count = 0;
        for (name, label) in names {
            let n = out.matches(name.as_str()).count();
            if n > 0 {
                count += n;
                out = out.replace(name.as_str(), label);
            }
        }
        (out, count)
    }

    /// Puts the real names back into a model's answer, for the user only.
    pub fn reveal(&self, text: &str) -> String {
        let mut labels: Vec<(&String, &String)> = self.reveal.iter().collect();
        // "Customer AB" before "Customer A".
        labels.sort_by_key(|(l, _)| std::cmp::Reverse(l.len()));
        let mut out = text.to_owned();
        for (label, name) in labels {
            out = out.replace(label.as_str(), name);
        }
        out
    }
}

/// What the gate did to one payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateReport {
    /// Identifiers withheld, by kind, with counts.
    pub withheld: BTreeMap<Withheld, usize>,
    /// Names replaced by pseudonyms.
    pub pseudonymised: usize,
    /// Fields dropped because the scope doesn't cover them.
    pub dropped_fields: BTreeSet<String>,
}

impl GateReport {
    /// Adds another report's counts to this one.
    pub fn merge(&mut self, other: GateReport) {
        for (k, n) in other.withheld {
            *self.withheld.entry(k).or_default() += n;
        }
        self.pseudonymised += other.pseudonymised;
        self.dropped_fields.extend(other.dropped_fields);
    }

    /// One line per thing withheld, for the register.
    pub fn summary(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .withheld
            .iter()
            .map(|(k, n)| format!("{} ×{n}", k.label()))
            .collect();
        if self.pseudonymised > 0 {
            out.push(format!(
                "Counterparty names → pseudonyms ×{}",
                self.pseudonymised
            ));
        }
        for f in &self.dropped_fields {
            out.push(format!("Field “{f}” (outside the task's scope)"));
        }
        out
    }
}

/// JSON keys whose values belong to a field class.
fn class_of(key: &str) -> Option<FieldClass> {
    match key {
        "memo" | "reference" | "message" | "note" | "description" => Some(FieldClass::LineMemos),
        "pdfBase64" | "xml" | "document" | "documents" | "lines_text" => {
            Some(FieldClass::Documents)
        }
        "counterpartyAccount"
        | "counterparty_account"
        | "iban"
        | "bic"
        | "ico"
        | "dic"
        | "vatId" => {
            // Never needed by a model; dropped like out-of-scope fields.
            Some(FieldClass::Documents)
        }
        _ => None,
    }
}

/// The gate for one run.
#[derive(Debug, Clone)]
pub struct Gate {
    scope: BTreeSet<FieldClass>,
    pseudonyms: Pseudonyms,
}

impl Gate {
    /// A gate for a task with `scope`.
    pub fn new(scope: impl IntoIterator<Item = FieldClass>, pseudonyms: Pseudonyms) -> Self {
        Self {
            scope: scope.into_iter().collect(),
            pseudonyms,
        }
    }

    /// The task's scope.
    pub fn scope(&self) -> &BTreeSet<FieldClass> {
        &self.scope
    }

    /// The pseudonyms in force (to reveal names in the answer locally).
    pub fn pseudonyms(&self) -> &Pseudonyms {
        &self.pseudonyms
    }

    /// Gates free text (a prompt).
    pub fn text(&self, text: &str) -> (String, GateReport) {
        let mut report = GateReport::default();
        let (mut out, withheld) = redact(text);
        for w in withheld {
            *report.withheld.entry(w).or_default() += 1;
        }
        if !self.scope.contains(&FieldClass::CounterpartyNames) {
            let (hidden, n) = self.pseudonyms.hide(&out);
            out = hidden;
            report.pseudonymised = n;
        }
        (out, report)
    }

    /// Gates a JSON value (a tool result): every string through [`Self::text`],
    /// and out-of-scope fields dropped.
    pub fn json(&self, value: &Value) -> (Value, GateReport) {
        let mut report = GateReport::default();
        let out = self.walk(value, &mut report);
        (out, report)
    }

    fn walk(&self, value: &Value, report: &mut GateReport) -> Value {
        match value {
            Value::String(s) => {
                let (t, r) = self.text(s);
                report.merge(r);
                Value::String(t)
            }
            Value::Array(items) => {
                Value::Array(items.iter().map(|v| self.walk(v, report)).collect())
            }
            Value::Object(map) => {
                let mut out = serde_json::Map::new();
                for (k, v) in map {
                    if let Some(class) = class_of(k)
                        && !self.scope.contains(&class)
                    {
                        report.dropped_fields.insert(k.clone());
                        continue;
                    }
                    // A key can carry an identifier too.
                    let (key, r) = self.text(k);
                    report.merge(r);
                    out.insert(key, self.walk(v, report));
                }
                Value::Object(out)
            }
            other => other.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parties() -> Pseudonyms {
        Pseudonyms::new(&[
            ("Northwind Traders s.r.o.".into(), Role::Customer),
            ("Acme Analytics a.s.".into(), Role::Customer),
            ("Pixelfarm s.r.o.".into(), Role::Vendor),
        ])
    }

    #[test]
    fn pseudonyms_are_stable_and_reversible() {
        let p = parties();
        let (t, n) =
            p.hide("Northwind Traders s.r.o. paid; Pixelfarm s.r.o. invoiced Acme Analytics a.s.");
        assert_eq!(t, "Customer B paid; Vendor A invoiced Customer A");
        assert_eq!(n, 3);
        assert_eq!(
            p.reveal("Customer B is late"),
            "Northwind Traders s.r.o. is late"
        );
        assert_eq!(parties(), p);
        // The short form is the same party.
        assert_eq!(p.hide("Pixelfarm invoiced").0, "Vendor A invoiced");
        assert_eq!(letters(25), "Z");
        assert_eq!(letters(26), "AA");
    }

    #[test]
    fn json_is_gated_by_scope() {
        let line = json!({
            "counterparty": "Pixelfarm s.r.o.",
            "counterpartyAccount": "CZ65 0800 0000 1920 0014 5399",
            "reference": "VS 2026114 for Pixelfarm s.r.o.",
            "amount": { "minor": -2_613_600, "currency": "CZK" }
        });
        let narrow = Gate::new([FieldClass::Aggregates], parties());
        let (out, report) = narrow.json(&line);
        assert_eq!(
            out,
            json!({ "counterparty": "Vendor A", "amount": { "minor": -2_613_600, "currency": "CZK" } })
        );
        assert_eq!(report.pseudonymised, 1);
        assert_eq!(report.dropped_fields.len(), 2);
        let wide = Gate::new(
            [FieldClass::CounterpartyNames, FieldClass::LineMemos],
            parties(),
        );
        let (out, _) = wide.json(&line);
        assert_eq!(out["counterparty"], "Pixelfarm s.r.o.");
        assert_eq!(out["reference"], "VS 2026114 for Pixelfarm s.r.o.");
        assert!(
            out.get("counterpartyAccount").is_none(),
            "account numbers never go"
        );
    }
}
