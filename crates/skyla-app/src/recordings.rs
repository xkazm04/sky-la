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
    /// The session gate, for the first-run flow.
    gate: &'a crate::session::Gate,
    name: &'static str,
    state: Option<String>,
    out: Vec<Recording>,
}

impl Scenario<'_> {
    fn call(&mut self, command: &str, args: Value, leads_to: Option<String>) -> Value {
        let (result, is_error) = match dispatch_session(self.gate, command, &args)
            .unwrap_or_else(|| dispatch(self.core, command, &args))
        {
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

/// The synthetic Pohoda export the invoice-import e2e test reads.
pub const POHODA_SAMPLE: &str =
    include_str!("../../../packages/fixtures/data/imports/pohoda-faktury.xml");

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

/// The received invoice the purchase e2e test types.
pub fn scripted_purchase() -> Value {
    json!({
        "supplier": "Kancelářské potřeby Novotný s.r.o.",
        "ico": "26965313",
        "dic": "CZ26965313",
        "number": "FP-2026-1187",
        "issueDate": "2026-10-03",
        "taxPointDate": "",
        "dueDate": "2026-10-17",
        "lines": [
            { "description": "Monitor", "account": "501", "vatCode": "IN21", "base": "12 000,00" },
            { "description": "Papír", "account": "501", "vatCode": "IN21", "base": "800,00" }
        ],
        "statedVat": "2 688,00"
    })
}

/// A scenario's script.
pub type Script = fn(&mut Scenario<'_>);

/// Every scenario, each recorded on a fresh demo core.
pub fn scenarios() -> Vec<(&'static str, Script)> {
    vec![
        // Asking the tax advisor: it asks first, then explains the books
        // and then the review's projection; each run is recorded.
        ("tax-advisor", |s| {
            s.read(
                "run_tax_advisor",
                json!({ "projection": null, "confirmed": false }),
            );
            s.write(
                "run_tax_advisor",
                json!({ "projection": null, "confirmed": true }),
                "books",
            );
            s.read("egress_register", json!({}));
            s.read("proposals", json!({}));
            s.read(
                "run_tax_advisor",
                json!({ "projection": review_projection(), "confirmed": false }),
            );
            s.write(
                "run_tax_advisor",
                json!({ "projection": review_projection(), "confirmed": true }),
                "review",
            );
            s.read("egress_register", json!({}));
            s.read("proposals", json!({}));
            s.read("egress_payload", json!({ "id": "run-2026-10-07-05" }));
        }),
        // "Explain this" on account 518 for Q3, then what was shared.
        ("explain-518", |s| {
            s.write(
                "explain",
                json!({ "target": explain_518(), "confirmed": true }),
                "explained",
            );
            s.read("egress_register", json!({}));
            s.read("egress_payload", json!({ "id": "run-2026-10-07-04" }));
        }),
        // First run: set up books, confirm the recovery key, unlock with the
        // passphrase (a wrong one first), then recover with the recovery key.
        ("first-run", |s| {
            s.read(
                "create_entity",
                json!({ "setup": first_run_setup(), "passphrase": "short" }),
            );
            let shown = s.write(
                "create_entity",
                json!({ "setup": first_run_setup(), "passphrase": FIRST_RUN_PASSPHRASE }),
                "created",
            );
            let key = shown["key"].as_str().unwrap_or_default().to_owned();
            let last = key.rsplit('-').next().unwrap_or_default().to_owned();
            s.read("confirm_recovery_key", json!({ "lastGroup": "AAAA" }));
            s.write(
                "confirm_recovery_key",
                json!({ "lastGroup": last }),
                "confirmed",
            );
            s.read(
                "unlock",
                json!({ "passphrase": "not the passphrase", "remember": false }),
            );
            s.write(
                "unlock",
                json!({ "passphrase": FIRST_RUN_PASSPHRASE, "remember": false }),
                "unlocked",
            );
            let fresh = s.write(
                "recover",
                json!({ "recoveryKey": key, "newPassphrase": "a brand new passphrase here" }),
                "recovered",
            );
            let last = fresh["key"]
                .as_str()
                .unwrap_or_default()
                .rsplit('-')
                .next()
                .unwrap_or_default()
                .to_owned();
            s.write(
                "confirm_recovery_key",
                json!({ "lastGroup": last }),
                "recovery-confirmed",
            );
        }),
        // The user stops the tax advisor from running at all.
        ("egress-policy", |s| {
            s.write(
                "set_egress_policy",
                json!({ "task": "tax.scenarios", "policy": "never" }),
                "tax-never",
            );
            s.read("egress_policies", json!({}));
        }),
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
        ("update-check", |s| {
            s.write("set_update_check", json!({ "enabled": true }), "on");
            s.read("update_status", json!({}));
            s.read("check_for_update", json!({}));
            s.write("set_update_check", json!({ "enabled": false }), "off");
            s.read("update_status", json!({}));
        }),
        ("new-customer", |s| {
            let mut draft = scripted_draft();
            draft["client"] = json!("");
            // The editor shows every problem with a new customer at once.
            draft["newClient"] = json!({ "name": "Northwind Traders s.r.o.", "ico": "12345678", "dic": "12", "address": "" });
            s.read("create_invoice_draft", json!({ "draft": draft.clone() }));
            draft["newClient"] = json!({
                "name": "Lesní ateliér s.r.o.",
                "ico": "26965313",
                "dic": "CZ26965313",
                "address": "Jasmínová 12, 106 00 Praha 10"
            });
            s.write("create_invoice_draft", json!({ "draft": draft }), "drafted");
            s.read("invoices", json!({}));
            s.read("invoice_form", json!({}));
        }),
        ("purchase", |s| {
            let mut draft = scripted_purchase();
            draft["dic"] = json!(null);
            draft["statedVat"] = json!("2 700,00");
            // Every problem at once: deducting VAT without the supplier's DIČ.
            s.read("record_purchase", json!({ "draft": draft }));
            s.write(
                "record_purchase",
                json!({ "draft": scripted_purchase() }),
                "recorded",
            );
            s.read("purchases", json!({}));
            s.read("integrity", json!({}));
        }),
        ("recurring", |s| {
            let mut draft = scripted_draft();
            draft["lines"] = json!([
                { "description": "Správa webu · {month}", "quantity": "1", "unit": "ks", "unitPrice": "6 000,00", "vatCode": "OUT21" }
            ]);
            let templates = s.write(
                "create_recurring",
                json!({ "recurring": {
                    "name": "Správa webu", "draft": draft, "frequency": "monthly",
                    "interval": 1, "start": "2026-11-01", "autoIssue": false
                } }),
                "templated",
            );
            s.read("recurring_templates", json!({}));
            let id = templates
                .as_array()
                .and_then(|t| t.iter().find(|x| x["name"] == "Správa webu"))
                .map(|t| t["id"].clone())
                .unwrap_or_default();
            s.write(
                "set_recurring_active",
                json!({ "id": id, "active": false }),
                "paused",
            );
            s.read("recurring_templates", json!({}));
        }),
        // Approving in the inbox: one likely posting, then the certain ones
        // together; each books its bank line.
        ("inbox-approve", |s| {
            s.write("approve_proposals", json!({ "ids": ["p-alza"] }), "one");
            s.read("proposals", json!({}));
            s.read("bank_statement", json!({}));
            s.read("integrity", json!({}));
            s.read("approve_proposals", json!({ "ids": ["p-alza"] }));
            s.write(
                "approve_proposals",
                json!({ "ids": ["p-google", "p-bank-fee"] }),
                "certain",
            );
            s.read("proposals", json!({}));
            s.read("bank_statement", json!({}));
            s.read("integrity", json!({}));
        }),
        // One certain line accepted, then its booking undone by a reversal.
        ("bank-undo", |s| {
            s.write("accept_bank_line", json!({ "line": "s1-1" }), "accepted");
            s.read("bank_statement", json!({}));
            s.read("purchases", json!({}));
            s.write("unbook_bank_line", json!({ "line": "s1-1" }), "undone");
            s.read("bank_statement", json!({}));
            s.read("purchases", json!({}));
            s.read("integrity", json!({}));
            s.read(
                "journal",
                json!({ "from": "2026-10-01", "to": "2026-10-31" }),
            );
        }),
        // Dismissing a finding that's been read.
        ("inbox-dismiss", |s| {
            s.write(
                "dismiss_proposal",
                json!({ "id": "finding-duplicate:figma:42" }),
                "dismissed",
            );
            s.read("proposals", json!({}));
        }),
        ("invoice-import", |s| {
            use base64::Engine as _;
            let file = json!({
                "fileName": "pohoda-faktury.xml",
                "contentBase64": base64::engine::general_purpose::STANDARD.encode(POHODA_SAMPLE.as_bytes()),
            });
            s.read("preview_invoice_import", file.clone());
            s.write("commit_invoice_import", file, "imported");
            s.read("invoices", json!({}));
            s.read("integrity", json!({}));
            s.read("balance_sheet", json!({ "asOf": "2026-10-07" }));
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
        ("export_books", json!({ "passphrase": null })),
        ("update_status", json!({})),
        ("profile", json!({})),
        ("reporting_periods", json!({})),
        ("purchase_form", json!({})),
        ("purchases", json!({})),
        ("egress_policies", json!({})),
        ("egress_payload", json!({ "id": "run-2026-10-04-01" })),
        ("egress_payload", json!({ "id": "run-2026-10-04-02" })),
        ("egress_payload", json!({ "id": "run-2026-10-06-01" })),
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
        ("advisor_status", json!({})),
        ("session_state", json!({})),
        ("backups", json!({})),
        ("financial_findings", json!({})),
        (
            "explain",
            json!({ "target": explain_518(), "confirmed": false }),
        ),
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
        "egress_register" => to_value(core.egress_register()),
        "egress_payload" => to_value(core.egress_payload(arg(args, "id")?)),
        "egress_policies" => to_value(Ok(core.egress_policies())),
        "set_egress_policy" => {
            to_value(core.set_egress_policy(arg(args, "task")?, arg(args, "policy")?))
        }
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
        "accept_bank_line" => to_value(core.accept_bank_line(arg(args, "line")?)),
        "unbook_bank_line" => to_value(core.unbook_bank_line(arg(args, "line")?)),
        "approve_proposals" => {
            let ids = args
                .get("ids")
                .cloned()
                .and_then(|d| serde_json::from_value::<Vec<String>>(d).ok())
                .ok_or_else(|| IpcFailure {
                    code: "bad_request".into(),
                    message: "missing or malformed argument ids".into(),
                })?;
            to_value(core.approve_proposals(&ids))
        }
        "dismiss_proposal" => to_value(core.dismiss_proposal(arg(args, "id")?)),
        "preview_invoice_import" => to_value(
            core.preview_invoice_import(arg(args, "fileName")?, arg(args, "contentBase64")?),
        ),
        "commit_invoice_import" => to_value(
            core.commit_invoice_import(arg(args, "fileName")?, arg(args, "contentBase64")?),
        ),
        "export_books" => to_value(core.export_books()),
        "reporting_periods" => to_value(core.reporting_periods()),
        "create_recurring" => {
            let r = args
                .get("recurring")
                .cloned()
                .and_then(|d| serde_json::from_value::<crate::dto::RecurringDraftDto>(d).ok())
                .ok_or_else(|| IpcFailure {
                    code: "bad_request".into(),
                    message: "missing or malformed argument recurring".into(),
                })?;
            to_value(core.create_recurring(&r))
        }
        "set_recurring_active" => {
            let active = args
                .get("active")
                .and_then(Value::as_bool)
                .ok_or_else(|| IpcFailure {
                    code: "bad_request".into(),
                    message: "missing or malformed argument active".into(),
                })?;
            to_value(core.set_recurring_active(id_arg(args)?, active))
        }
        "profile" => to_value(Ok(core.profile())),
        "purchase_form" => to_value(core.purchase_form()),
        "purchases" => to_value(core.purchases()),
        "record_purchase" => {
            let draft = args
                .get("draft")
                .cloned()
                .and_then(|d| serde_json::from_value::<crate::dto::PurchaseDraftDto>(d).ok())
                .ok_or_else(|| IpcFailure {
                    code: "bad_request".into(),
                    message: "missing or malformed argument draft".into(),
                })?;
            to_value(core.record_purchase(&draft))
        }
        "update_status" => to_value(Ok(core.update_status())),
        "set_update_check" => {
            let enabled = args
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| IpcFailure {
                    code: "bad_request".into(),
                    message: "missing or malformed argument enabled".into(),
                })?;
            to_value(Ok(core.set_update_check(enabled)))
        }
        "check_for_update" => to_value(core.check_for_update()),
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
        "session_state" => to_value(Ok(core.session_state())),
        "backups" => to_value(core.backups()),
        "advisor_status" => to_value(Ok(core.advisor_status())),
        "financial_findings" => to_value(core.financial_findings()),
        "explain" => {
            let target: crate::dto::ExplainTargetDto =
                serde_json::from_value(args["target"].clone())
                    .map_err(|e| crate::CoreError::BadRequest(e.to_string()))?;
            let confirmed = args
                .get("confirmed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            to_value(core.explain(&target, confirmed))
        }
        "run_tax_advisor" => {
            let projection: Option<crate::dto::TaxProjectionDto> = match args.get("projection") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    serde_json::from_value(v.clone())
                        .map_err(|e| crate::CoreError::BadRequest(e.to_string()))?,
                ),
            };
            let confirmed = args
                .get("confirmed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            to_value(core.run_tax_advisor(projection.as_ref(), confirmed))
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
        // A reproducible gate in its own folder, as the Tauri test builds it.
        let dir =
            std::env::temp_dir().join(format!("skyla-recording-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let gate = crate::session::Gate::reproducible(dir.clone());
        let mut s = Scenario {
            core: &core,
            gate: &gate,
            name,
            state: None,
            out: Vec::new(),
        };
        run(&mut s);
        out.extend(s.out);
        let _ = std::fs::remove_dir_all(&dir);
    }
    out
}

/// Session commands (setup, unlock, recovery) on the gate; `None` for the
/// rest, which the core answers.
pub fn dispatch_session(
    gate: &crate::session::Gate,
    command: &str,
    args: &Value,
) -> Option<Result<Value, IpcFailure>> {
    let text = |k: &str| {
        args.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let failure = |e: crate::CoreError| IpcFailure::from(e);
    let open = |c: Core| crate::dto::SessionStateDto {
        state: "open".into(),
        entity: Some(c.entity().display_name),
        remembered: false,
    };
    let result = match command {
        "create_entity" => {
            let setup: crate::dto::EntitySetupDto =
                match serde_json::from_value(args["setup"].clone()) {
                    Ok(s) => s,
                    Err(e) => {
                        return Some(Err(failure(crate::CoreError::BadRequest(e.to_string()))));
                    }
                };
            gate.create(&setup, &text("passphrase"))
                .map(|(_, k)| serde_json::to_value(k).unwrap_or(Value::Null))
        }
        "confirm_recovery_key" => gate
            .confirm_recovery_key(&text("lastGroup"))
            .map(Value::Bool),
        "unlock" => gate
            .unlock(
                &text("passphrase"),
                args.get("remember")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            )
            .map(|c| serde_json::to_value(open(c)).unwrap_or(Value::Null)),
        "recover" => gate
            .recover(&text("recoveryKey"), &text("newPassphrase"))
            .map(|(_, k)| serde_json::to_value(k).unwrap_or(Value::Null)),
        _ => return None,
    };
    Some(result.map_err(failure))
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

/// Account 518 for Q3 2026, the demo's "explain this".
fn explain_518() -> Value {
    json!({ "kind": "account", "account": "518", "from": "2026-07-01", "to": "2026-09-30", "entry": null })
}

/// The first-run scenario's passphrase, typed in the e2e test.
pub const FIRST_RUN_PASSPHRASE: &str = "a long passphrase for the books";

/// The first-run form as the e2e test fills it.
pub fn first_run_setup() -> Value {
    json!({
        "displayName": "Eva Malá",
        "ico": "27415830",
        "dic": "CZ8001011234",
        "address": "Dlouhá 1, 110 00 Praha 1",
        "vatPeriod": "monthly",
        "registration": "Zapsána v živnostenském rejstříku",
        "iban": "CZ6508000000192000145399",
        "bankName": "ČSOB",
        "email": null,
        "flatRateGroup": "liberal",
    })
}
