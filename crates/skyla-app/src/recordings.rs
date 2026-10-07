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
    ];
    for (from, to) in [
        ("2026-07-01", "2026-07-31"),
        ("2026-08-01", "2026-08-31"),
        ("2026-09-01", "2026-09-30"),
    ] {
        requests.push(("vat_return", json!({ "from": from, "to": to })));
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
        "vat_return" => to_value(core.vat_return(arg(args, "from")?, arg(args, "to")?)),
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
            }
        })
        .collect()
}
