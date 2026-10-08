//! IPC recordings: the core's answers to a fixed set of canonical requests,
//! committed as `packages/fixtures/data/ipc-recordings.json`. The webview's
//! mock transport replays them, so the UI in `dev:web` and Playwright sees
//! exactly what the real core returns. Tests keep the file current and
//! replay every recording over the real Tauri IPC (`apps/desktop/src-tauri`).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Core, IpcFailure};

/// Where the recordings live, relative to the repository root.
pub const RECORDINGS_PATH: &str = "packages/fixtures/data/ipc-recordings.json";

/// One request and its answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recording {
    /// Command name as invoked over IPC, e.g. `profit_and_loss`.
    pub command: String,
    /// Arguments as the webview sends them (camelCase keys).
    pub args: Value,
    /// The command's result, or the `IpcFailure` it returned.
    pub result: Value,
    /// True when `result` is an error.
    pub is_error: bool,
    /// The scenario this step belongs to; none for the demo's initial answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    /// The state the answer holds in; none for the initial state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// For a write: the state it leads to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leads_to: Option<String>,
}

/// Records a scenario: writes move the demo from state to state, and the
/// reads after them are recorded in the state they hold in. The mock replays
/// a write only from its recorded state, so a scripted flow (create, issue,
/// export) runs end to end without the webview computing anything.
pub struct Scenario<'a> {
    core: &'a Core,
    name: &'static str,
    state: Option<String>,
    out: Vec<Recording>,
}

impl Scenario<'_> {
    fn call(&mut self, command: &str, args: Value, leads_to: Option<String>) -> Value {
        let (result, is_error) = match dispatch(self.core, command, &args) {
            Ok(value) => (value, false),
            Err(failure) => (serde_json::to_value(failure).unwrap_or(Value::Null), true),
        };
        let leads_to = leads_to.filter(|_| !is_error);
        self.out.push(Recording {
            command: command.to_owned(),
            args,
            result: result.clone(),
            is_error,
            scenario: Some(self.name.to_owned()),
            state: self.state.clone(),
            leads_to: leads_to.clone(),
        });
        if let Some(next) = leads_to {
            self.state = Some(next);
        }
        result
    }

    /// A read in the current state.
    pub fn read(&mut self, command: &str, args: Value) -> Value {
        self.call(command, args, None)
    }

    /// A write that moves to `state` (`<scenario>/<state>`) when it succeeds.
    pub fn write(&mut self, command: &str, args: Value, state: &str) -> Value {
        let next = format!("{}/{state}", self.name);
        self.call(command, args, Some(next))
    }
}

/// The second October statement, which the workbench's e2e test imports.
pub const SECOND_STATEMENT: &str =
    include_str!("../../../packages/fixtures/data/statements/csob-2026-10-07.xml");

/// The demo's ČNB rates file, imported in the reference-data scenario.
pub const DEMO_CNB_DAILY: &str =
    include_str!("../../../packages/fixtures/data/refdata/demo-cnb-daily-2026-10-07.txt");
/// The demo's repo-rate history.
pub const DEMO_CNB_REPO: &str =
    include_str!("../../../packages/fixtures/data/refdata/demo-cnb-repo-history.csv");

/// The editor's scripted invoice: typed in the e2e test exactly like this.
pub fn scripted_draft() -> Value {
    json!({
        "client": "Northwind Traders s.r.o.",
        "dueDays": 14,
        "note": "Děkuji za spolupráci.",
        "lines": [
            { "description": "UX audit", "quantity": "12", "unit": "h", "unitPrice": "1 450,00", "vatCode": "OUT21" },
            { "description": "Workshop", "quantity": "1", "unit": "ks", "unitPrice": "8 000,00", "vatCode": "OUT21" }
        ]
    })
}

/// A scenario's script.
pub type Script = fn(&mut Scenario<'_>);

/// Every scenario, each recorded on a fresh demo core.
pub fn scenarios() -> Vec<(&'static str, Script)> {
    vec![
        // The scenario form refuses what it can't read, with the reason.
        ("tax-projection", |s| {
            s.read(
                "income_tax_scenarios",
                json!({ "projection": mistyped_projection() }),
            );
        }),
        ("new-invoice", |s| {
            s.read("invoice_form", json!({}));
            // The editor sends what was typed; the core lists every problem.
            s.read(
                "create_invoice_draft",
                json!({ "draft": { "client": "", "dueDays": 14, "note": "", "lines": [
                    { "description": "UX audit", "quantity": "12", "unit": "h", "unitPrice": "1450.00", "vatCode": "OUT21" }
                ] } }),
            );
            let draft = s.write(
                "create_invoice_draft",
                json!({ "draft": scripted_draft() }),
                "drafted",
            );
            let id = draft["id"].clone();
            s.read("invoices", json!({}));
            s.read("invoice_form", json!({}));
            let issued = s.write(
                "issue_invoice",
                json!({ "id": id, "issueDate": "2026-10-07" }),
                "issued",
            );
            s.read("invoices", json!({}));
            s.read("balance_sheet", json!({ "asOf": "2026-10-07" }));
            s.read("integrity", json!({}));
            s.read("invoice_pdf", json!({ "id": issued["id"], "lang": "cs" }));
            s.read(
                "invoice_xml",
                json!({ "id": issued["id"], "format": "isdoc" }),
            );
        }),
        ("bank-workbench", |s| {
            use base64::Engine as _;
            let file =
                base64::engine::general_purpose::STANDARD.encode(SECOND_STATEMENT.as_bytes());
            s.write(
                "import_bank_statement",
                json!({ "fileName": "csob-2026-10-07.xml", "contentBase64": file }),
                "imported",
            );
            s.read("bank_statement", json!({}));
            s.read("proposals", json!({}));
            s.write("accept_certain_bank_lines", json!({}), "accepted");
            s.read("bank_statement", json!({}));
            s.read("proposals", json!({}));
            s.read("invoices", json!({}));
            s.read("integrity", json!({}));
            s.read("balance_sheet", json!({ "asOf": "2026-10-07" }));
            s.write(
                "book_bank_line",
                json!({ "line": "s2-3", "allocations": [
                    { "entryId": null, "account": "501", "vatCode": "IN21", "amount": "3 630,00" },
                    { "entryId": null, "account": "518", "vatCode": "IN21", "amount": "1 210,00" }
                ] }),
                "split",
            );
            s.read("bank_statement", json!({}));
            s.read("proposals", json!({}));
            s.read("integrity", json!({}));
            s.write(
                "create_bank_rule",
                json!({ "line": "s2-2", "rule": {
                    "name": "Office rent", "account": "518", "vatCode": null, "autoAccept": true
                } }),
                "ruled",
            );
            s.read("bank_statement", json!({}));
            s.read("proposals", json!({}));
            s.read("integrity", json!({}));
        }),
        ("reference-data", |s| {
            s.write(
                "import_reference_data",
                json!({ "kind": "cnb_fx", "fileName": "demo-cnb-daily-2026-10-07.txt", "text": DEMO_CNB_DAILY }),
                "rates",
            );
            s.read("reference_data", json!({}));
            s.write(
                "import_reference_data",
                json!({ "kind": "cnb_repo", "fileName": "demo-cnb-repo-history.csv", "text": DEMO_CNB_REPO }),
                "repo",
            );
            s.read("reference_data", json!({}));
            s.read("dunning_queue", json!({ "asOf": "2026-10-07" }));
            s.write(
                "set_reference_fetch",
                json!({ "enabled": true }),
                "fetching",
            );
            s.read("reference_data", json!({}));
        }),
        ("discard-draft", |s| {
            let mut draft = scripted_draft();
            draft["client"] = json!("Acme Analytics a.s.");
            let created = s.write("create_invoice_draft", json!({ "draft": draft }), "drafted");
            s.read("invoices", json!({}));
            s.write(
                "delete_invoice_draft",
                json!({ "id": created["id"] }),
                "deleted",
            );
            s.read("invoices", json!({}));
        }),
    ]
}

/// Every canonical request. The UI asks for these exact arguments in
/// `dev:web`; anything else gets an error from the mock.
pub fn canonical_requests() -> Vec<(&'static str, Value)> {
    let quarters = [
        ("2026-04-01", "2026-06-30"),
        ("2026-07-01", "2026-09-30"),
        ("2026-10-01", "2026-12-31"),
        ("2026-04-01", "2026-09-30"),
    ];
    let mut requests: Vec<(&'static str, Value)> = vec![
        ("app_info", json!({})),
        ("entity", json!({})),
        ("periods", json!({})),
        ("integrity", json!({})),
        ("invoices", json!({})),
        ("bank_statement", json!({})),
        ("proposals", json!({})),
        ("egress_register", json!({})),
        ("rule_pack", json!({})),
        // The overdue invoice, for the PDF export in both languages.
        ("invoice_pdf", json!({ "id": 6, "lang": "cs" })),
        ("invoice_pdf", json!({ "id": 6, "lang": "en" })),
        ("invoice_xml", json!({ "id": 6, "format": "isdoc" })),
        ("invoice_xml", json!({ "id": 6, "format": "ubl" })),
        ("invoice_xml", json!({ "id": 6, "format": "cii" })),
        ("dunning_queue", json!({ "asOf": "2026-10-07" })),
        ("recurring_templates", json!({})),
        ("reference_data", json!({})),
        ("income_tax_scenarios", json!({ "projection": null })),
        ("obligations", json!({ "year": 2026 })),
        (
            "income_tax_scenarios",
            json!({ "projection": review_projection() }),
        ),
    ];
    for (from, to) in [
        ("2026-07-01", "2026-07-31"),
        ("2026-08-01", "2026-08-31"),
        ("2026-09-01", "2026-09-30"),
    ] {
        requests.push(("vat_return", json!({ "from": from, "to": to })));
        requests.push(("control_statement", json!({ "from": from, "to": to })));
    }
    for (from, to) in quarters {
        requests.push(("profit_and_loss", json!({ "from": from, "to": to })));
        requests.push(("cash_basis", json!({ "from": from, "to": to })));
        requests.push(("journal", json!({ "from": from, "to": to })));
    }
    for as_of in ["2026-06-30", "2026-09-30", "2026-10-07"] {
        requests.push(("balance_sheet", json!({ "asOf": as_of })));
    }
    requests.push(("trial_balance", json!({ "from": null, "to": "2026-09-30" })));
    requests.push((
        "trial_balance",
        json!({ "from": "2026-07-01", "to": "2026-09-30" }),
    ));
    requests
}

fn id_arg(args: &Value) -> Result<i64, IpcFailure> {
    args.get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| IpcFailure {
            code: "bad_request".into(),
            message: "missing argument id".into(),
        })
}

fn arg<'a>(args: &'a Value, name: &str) -> Result<&'a str, IpcFailure> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| IpcFailure {
            code: "bad_request".into(),
            message: format!("missing argument {name}"),
        })
}

fn to_value<T: Serialize>(result: Result<T, crate::CoreError>) -> Result<Value, IpcFailure> {
    let value = result.map_err(IpcFailure::from)?;
    serde_json::to_value(value).map_err(|e| IpcFailure {
        code: "serialize".into(),
        message: e.to_string(),
    })
}

/// Answers a request by name, as the Tauri command with that name does. Used
/// to record; the desktop shell calls the typed `Core` methods directly.
pub fn dispatch(core: &Core, command: &str, args: &Value) -> Result<Value, IpcFailure> {
    match command {
        "app_info" => to_value(Ok(core.app_info())),
        "entity" => to_value(Ok(core.entity())),
        "periods" => to_value(core.periods()),
        "integrity" => to_value(core.integrity()),
        "invoices" => to_value(core.invoices()),
        "bank_statement" => to_value(core.bank_statement()),
        "proposals" => to_value(core.proposals()),
        "egress_register" => to_value(Ok(core.egress_register())),
        "rule_pack" => to_value(Ok(core.rule_pack())),
        "recurring_templates" => to_value(core.recurring_templates()),
        "invoice_form" => to_value(core.invoice_form()),
        "create_invoice_draft" => {
            let draft = args
                .get("draft")
                .cloned()
                .and_then(|d| serde_json::from_value::<crate::dto::InvoiceDraftDto>(d).ok())
                .ok_or_else(|| IpcFailure {
                    code: "bad_request".into(),
                    message: "missing or malformed argument draft".into(),
                })?;
            to_value(core.create_invoice_draft(&draft))
        }
        "issue_invoice" => {
            let id = id_arg(args)?;
            to_value(core.issue_invoice(id, arg(args, "issueDate")?))
        }
        "delete_invoice_draft" => to_value(core.delete_invoice_draft(id_arg(args)?)),
        "import_bank_statement" => to_value(
            core.import_bank_statement(arg(args, "fileName")?, arg(args, "contentBase64")?),
        ),
        "accept_certain_bank_lines" => to_value(core.accept_certain_bank_lines()),
        "reference_data" => to_value(core.reference_data()),
        "import_reference_data" => to_value(core.import_reference_data(
            arg(args, "kind")?,
            arg(args, "fileName")?,
            arg(args, "text")?,
        )),
        "set_reference_fetch" => {
            let enabled = args
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| IpcFailure {
                    code: "bad_request".into(),
                    message: "missing argument enabled".into(),
                })?;
            to_value(core.set_reference_fetch(enabled))
        }
        "install_pack_update" => {
            to_value(core.install_pack_update(arg(args, "packToml")?, arg(args, "signature")?))
        }
        "book_bank_line" => {
            let allocations: Vec<crate::dto::BankAllocationDto> = args
                .get("allocations")
                .cloned()
                .and_then(|a| serde_json::from_value(a).ok())
                .ok_or_else(|| IpcFailure {
                    code: "bad_request".into(),
                    message: "missing or malformed argument allocations".into(),
                })?;
            to_value(core.book_bank_line(arg(args, "line")?, &allocations))
        }
        "create_bank_rule" => {
            let rule: crate::dto::BankRuleInputDto = args
                .get("rule")
                .cloned()
                .and_then(|r| serde_json::from_value(r).ok())
                .ok_or_else(|| IpcFailure {
                    code: "bad_request".into(),
                    message: "missing or malformed argument rule".into(),
                })?;
            to_value(core.create_bank_rule(arg(args, "line")?, &rule))
        }
        "dunning_queue" => to_value(core.dunning_queue(arg(args, "asOf")?)),
        "invoice_pdf" => {
            let id = args
                .get("id")
                .and_then(Value::as_i64)
                .ok_or_else(|| IpcFailure {
                    code: "bad_request".into(),
                    message: "missing argument id".into(),
                })?;
            to_value(core.invoice_pdf(id, arg(args, "lang")?))
        }
        "invoice_xml" => {
            let id = args
                .get("id")
                .and_then(Value::as_i64)
                .ok_or_else(|| IpcFailure {
                    code: "bad_request".into(),
                    message: "missing argument id".into(),
                })?;
            to_value(core.invoice_xml(id, arg(args, "format")?))
        }
        "vat_return" => to_value(core.vat_return(arg(args, "from")?, arg(args, "to")?)),
        "income_tax_scenarios" => {
            let projection: Option<crate::dto::TaxProjectionDto> = match args.get("projection") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    serde_json::from_value(v.clone())
                        .map_err(|e| crate::CoreError::BadRequest(e.to_string()))?,
                ),
            };
            to_value(core.income_tax_scenarios(projection.as_ref()))
        }
        "obligations" => {
            let year = args
                .get("year")
                .and_then(Value::as_i64)
                .and_then(|y| i32::try_from(y).ok())
                .ok_or_else(|| IpcFailure {
                    code: "bad_request".into(),
                    message: "year must be a number".into(),
                })?;
            to_value(core.obligations(year))
        }
        "control_statement" => {
            to_value(core.control_statement(arg(args, "from")?, arg(args, "to")?))
        }
        "profit_and_loss" => to_value(core.profit_and_loss(arg(args, "from")?, arg(args, "to")?)),
        "cash_basis" => to_value(core.cash_basis(arg(args, "from")?, arg(args, "to")?)),
        "journal" => to_value(core.journal(arg(args, "from")?, arg(args, "to")?)),
        "balance_sheet" => to_value(core.balance_sheet(arg(args, "asOf")?)),
        "trial_balance" => {
            let from = args.get("from").and_then(Value::as_str);
            to_value(core.trial_balance(from, arg(args, "to")?))
        }
        other => Err(IpcFailure {
            code: "bad_request".into(),
            message: format!("unknown command {other}"),
        }),
    }
}

/// Records the initial answers on `core`, then every scenario on a fresh
/// core from `fresh`.
pub fn record_all(core: &Core, fresh: impl Fn() -> Core) -> Vec<Recording> {
    let mut out = record(core);
    for (name, run) in scenarios() {
        let core = fresh();
        let mut s = Scenario {
            core: &core,
            name,
            state: None,
            out: Vec::new(),
        };
        run(&mut s);
        out.extend(s.out);
    }
    out
}

/// Records every canonical request.
pub fn record(core: &Core) -> Vec<Recording> {
    canonical_requests()
        .into_iter()
        .map(|(command, args)| {
            let (result, is_error) = match dispatch(core, command, &args) {
                Ok(value) => (value, false),
                Err(failure) => (serde_json::to_value(failure).unwrap_or(Value::Null), true),
            };
            Recording {
                command: command.to_owned(),
                args,
                result,
                is_error,
                scenario: None,
                state: None,
                leads_to: None,
            }
        })
        .collect()
}

/// The projection from the design review (1 571 000 income, 60 % flat
/// rate), with a laptop to time.
fn review_projection() -> Value {
    json!({
        "income": "1 571 000",
        "expenses": "383 200",
        "flatRate": "trade",
        "purchaseDescription": "Laptop",
        "purchasePrice": "60 000",
    })
}

/// The same with dots in the income, which the core refuses.
fn mistyped_projection() -> Value {
    let mut p = review_projection();
    p["income"] = json!("1.571.000");
    p
}
