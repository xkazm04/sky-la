//! The demo entity: the golden journal (`packages/fixtures/data/demo-ledger.json`)
//! posted into an in-memory ledger, plus typed data for the domains that
//! don't exist yet (`packages/fixtures/data/demo-domain.json`). The desktop
//! shell opens this until the unlock flow lands (WP-30).

use std::collections::HashMap;

use rusqlite::Connection;
use serde::Deserialize;
use skyla_ledger::{
    ChartSpec, NewEntry, NewLine, Replay, SourceKind, apply_schema, create_draft_as, get_entry,
    link_settlement, open_period, post_entry_at, reverse_entry_at, seed_chart,
    set_functional_currency,
};
use skyla_money::{Currency, Money, Rate};

use crate::CoreError;

const CZ_CHART: &str = include_str!("../../../rules/cz/chart.toml");
/// The golden journal shared with the ledger's projection tests.
pub const GOLDEN_JOURNAL: &str = include_str!("../../../packages/fixtures/data/demo-ledger.json");
/// Typed demo data for the domains not built yet.
pub const DOMAIN: &str = include_str!("../../../packages/fixtures/data/demo-domain.json");

#[derive(Deserialize)]
struct Golden {
    functional_currency: String,
    periods: Vec<(String, String)>,
    entries: Vec<GoldenEntry>,
}

#[derive(Deserialize)]
struct GoldenEntry {
    key: String,
    date: String,
    source: String,
    memo: String,
    #[serde(rename = "ref")]
    reference: Option<String>,
    lines: Vec<GoldenLine>,
    #[serde(default)]
    settles: Vec<GoldenSettle>,
    reverses: Option<String>,
}

#[derive(Deserialize)]
struct GoldenLine {
    account: String,
    amount_minor: i64,
    currency: Option<String>,
    functional_minor: Option<i64>,
    fx_rate: Option<String>,
    vat_code: Option<String>,
    memo: Option<String>,
}

#[derive(Deserialize)]
struct GoldenSettle {
    entry: String,
    amount_minor: i64,
}

fn source_kind(text: &str) -> Result<SourceKind, CoreError> {
    Ok(match text {
        "manual" => SourceKind::Manual,
        "invoice" => SourceKind::Invoice,
        "bank" => SourceKind::Bank,
        "opening" => SourceKind::Opening,
        "reversal" => SourceKind::Reversal,
        other => return Err(CoreError::Demo(format!("unknown source {other}"))),
    })
}

/// Builds the demo ledger: schema, CZ chart, periods and every golden entry,
/// posted through the kernel (so every invariant and the hash chain apply).
pub fn demo_ledger() -> Result<Connection, CoreError> {
    let golden: Golden =
        serde_json::from_str(GOLDEN_JOURNAL).map_err(|e| CoreError::Demo(e.to_string()))?;
    let conn = Connection::open_in_memory().map_err(skyla_ledger::LedgerError::from)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(skyla_ledger::LedgerError::from)?;
    apply_schema(&conn)?;
    seed_chart(&conn, &ChartSpec::from_toml(CZ_CHART)?)?;
    set_functional_currency(&conn, Currency::from_code(&golden.functional_currency)?)?;
    for (start, end) in &golden.periods {
        open_period(&conn, start, end)?;
    }
    let mut ids: HashMap<String, i64> = HashMap::new();
    let id_of = |ids: &HashMap<String, i64>, key: &str| {
        ids.get(key)
            .copied()
            .ok_or_else(|| CoreError::Demo(format!("unknown entry {key}")))
    };
    for (index, entry) in golden.entries.iter().enumerate() {
        // Identities and posting times are fixed, so the hash chain (and with
        // it every report snapshot) is identical on every machine and run.
        let uid = format!("00000000-0000-7000-8000-{:012}", index + 1);
        let posted_at = format!("{}T18:00:00.000Z", entry.date);
        let id = if let Some(original) = &entry.reverses {
            let replay = Replay {
                uid: &uid,
                posted_at: &posted_at,
            };
            reverse_entry_at(
                &conn,
                id_of(&ids, original)?,
                &entry.date,
                "user",
                Some(&entry.memo),
                Some(replay),
            )?
            .entry_id
        } else {
            let lines = entry
                .lines
                .iter()
                .map(|l| {
                    let currency = Currency::from_code(
                        l.currency.as_deref().unwrap_or(&golden.functional_currency),
                    )?;
                    let conversion = match (&l.fx_rate, l.functional_minor) {
                        (Some(rate), Some(functional)) => Some((
                            Money::new(
                                functional,
                                Currency::from_code(&golden.functional_currency)?,
                            ),
                            rate.parse::<Rate>()
                                .map_err(|e| CoreError::Demo(e.to_string()))?,
                        )),
                        _ => None,
                    };
                    Ok(NewLine {
                        account: l.account.clone(),
                        amount: Money::new(l.amount_minor, currency),
                        conversion,
                        vat_code: l.vat_code.clone(),
                        tax_treatment: None,
                        memo: l.memo.clone().unwrap_or_default(),
                    })
                })
                .collect::<Result<Vec<_>, CoreError>>()?;
            let id = create_draft_as(
                &conn,
                &NewEntry {
                    date: entry.date.clone(),
                    source_kind: source_kind(&entry.source)?,
                    source_ref: entry.reference.clone(),
                    memo: entry.memo.clone(),
                    created_by: "user".into(),
                    lines,
                },
                &uid,
            )?;
            for s in &entry.settles {
                let functional = Currency::from_code(&golden.functional_currency)?;
                link_settlement(
                    &conn,
                    id,
                    id_of(&ids, &s.entry)?,
                    Money::new(s.amount_minor, functional),
                )?;
            }
            post_entry_at(&conn, id, None, Some(&posted_at))?;
            id
        };
        debug_assert!(get_entry(&conn, id).is_ok());
        ids.insert(entry.key.clone(), id);
    }
    Ok(conn)
}

/// Records the demo's invoices in the invoicing module: issued ones are
/// imported against their golden-journal entries (which checks every line
/// total against the ledger), drafts stay drafts. Returns the scheduled
/// drafts' issue dates by document id.
pub(crate) fn seed_invoicing(
    conn: &Connection,
    pack: &skyla_rules::Pack,
    domain: &Domain,
) -> Result<HashMap<i64, String>, CoreError> {
    use skyla_invoicing::{Accounts, DocKind, DraftInput, define_series};
    skyla_invoicing::apply_schema(conn)?;
    define_series(conn, "FV", DocKind::Invoice, "{YYYY}-{NNN}", "Faktury")?;
    define_series(
        conn,
        "OD",
        DocKind::CreditNote,
        "OD{YYYY}-{NNN}",
        "Opravné daňové doklady",
    )?;
    define_series(
        conn,
        "ZF",
        DocKind::Advance,
        "ZF{YYYY}-{NN}",
        "Zálohové faktury",
    )?;
    define_series(
        conn,
        "DZ",
        DocKind::AdvanceTax,
        "DZ{YYYY}-{NN}",
        "Daňové doklady k přijatým platbám",
    )?;
    let s = &domain.supplier;
    skyla_invoicing::set_supplier(
        conn,
        &skyla_invoicing::Supplier {
            name: s.name.clone(),
            ico: s.ico.clone(),
            dic: s.dic.clone(),
            address: s.address.clone(),
            iban: s.iban.clone(),
            bic: s.bic.clone(),
            email: s.email.clone(),
            vat_payer: s.vat_payer,
            registration: s.registration.clone(),
        },
    )?;
    let accounts = Accounts::cz();
    let mut scheduled = HashMap::new();
    for (i, doc) in domain.invoices.iter().enumerate() {
        let input = DraftInput {
            kind: DocKind::Invoice,
            series: "FV".into(),
            customer: customer(domain, &doc.client)?,
            due_date: doc.due_on.clone(),
            tax_point_date: None,
            note: String::new(),
            lines: line_inputs(&doc.lines),
            related_id: None,
            advances: Vec::new(),
        };
        match &doc.state {
            None => {
                let entry_id =
                    skyla_ledger::find_posted_by_ref(conn, SourceKind::Invoice, &doc.number)?
                        .ok_or_else(|| {
                            CoreError::Demo(format!("invoice {} has no posted entry", doc.number))
                        })?;
                let date = get_entry(conn, entry_id)?.date;
                skyla_invoicing::import_issued(
                    conn,
                    pack,
                    &accounts,
                    &input,
                    &doc.number,
                    &date,
                    entry_id,
                )?;
            }
            Some(_) => {
                // A fixed id per demo draft, so recordings come out the same.
                let uid = format!("00000000-0000-7000-8000-{:012}", i + 1);
                let id = skyla_invoicing::create_draft_as(conn, &input, &uid)?;
                if let Some(when) = &doc.scheduled_for {
                    scheduled.insert(id, when.clone());
                }
            }
        }
    }
    for t in &domain.recurring {
        skyla_invoicing::recurring::create_template(
            conn,
            &skyla_invoicing::recurring::TemplateInput {
                name: t.name.clone(),
                draft: DraftInput {
                    kind: DocKind::Invoice,
                    series: "FV".into(),
                    customer: customer(domain, &t.client)?,
                    due_date: None,
                    tax_point_date: None,
                    note: String::new(),
                    lines: line_inputs(&t.lines),
                    related_id: None,
                    advances: Vec::new(),
                },
                schedule: skyla_invoicing::recurring::Schedule {
                    frequency: t.frequency,
                    interval: t.interval,
                    start: t.start.clone(),
                    end: None,
                },
                due_days: t.due_days,
                auto_issue: t.auto_issue,
            },
        )?;
    }
    Ok(scheduled)
}

/// The customer an invoice names, with its IČO checked.
fn customer(domain: &Domain, name: &str) -> Result<skyla_invoicing::Customer, CoreError> {
    let client = domain
        .clients
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| CoreError::Demo(format!("unknown client {name}")))?;
    if !skyla_invoicing::valid_ico(&client.ico) {
        return Err(CoreError::Demo(format!(
            "client {}: invalid IČO {}",
            client.name, client.ico
        )));
    }
    Ok(skyla_invoicing::Customer {
        name: client.legal_name.clone(),
        ico: Some(client.ico.clone()),
        dic: client.dic.clone(),
        address: Some(client.address.clone()),
    })
}

fn line_inputs(lines: &[DomainInvoiceLine]) -> Vec<skyla_invoicing::LineInput> {
    lines
        .iter()
        .map(|l| skyla_invoicing::LineInput {
            description: l.description.clone(),
            quantity: l.quantity.clone(),
            unit: l.unit.clone(),
            unit_price_minor: l.unit_price_minor,
            vat_code: l.vat_code.clone(),
            account: None,
        })
        .collect()
}

/// `demo-domain.json`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Domain {
    /// The entity.
    pub(crate) entity: DomainEntity,
    /// The supplier profile printed on documents.
    pub(crate) supplier: DomainSupplier,
    /// Customers, by the name invoices use.
    pub(crate) clients: Vec<DomainClient>,
    /// Recurring invoice templates.
    pub(crate) recurring: Vec<DomainRecurring>,
    /// Invoice documents (issued ones link to the ledger by number).
    pub(crate) invoices: Vec<DomainInvoice>,
    /// The latest bank import.
    pub(crate) bank_import: DomainBankImport,
    /// The inbox.
    pub(crate) proposals: Vec<DomainProposal>,
    /// The egress register.
    pub(crate) egress_runs: Vec<crate::dto::EgressRunDto>,
}

/// A recurring invoice template.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainRecurring {
    pub(crate) name: String,
    pub(crate) client: String,
    pub(crate) frequency: skyla_invoicing::recurring::Frequency,
    pub(crate) interval: u32,
    pub(crate) start: String,
    pub(crate) due_days: u16,
    pub(crate) auto_issue: bool,
    pub(crate) lines: Vec<DomainInvoiceLine>,
}

/// A customer.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainClient {
    pub(crate) name: String,
    pub(crate) legal_name: String,
    pub(crate) ico: String,
    pub(crate) dic: Option<String>,
    pub(crate) address: String,
}

/// The entity's settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainEntity {
    pub(crate) display_name: String,
    pub(crate) legal_form: String,
    pub(crate) vat_period: String,
    pub(crate) functional_currency: String,
    pub(crate) as_of: String,
    pub(crate) bank_account: String,
    pub(crate) bank_name: String,
}

/// The supplier profile.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainSupplier {
    pub(crate) name: String,
    pub(crate) ico: Option<String>,
    pub(crate) dic: Option<String>,
    pub(crate) address: String,
    pub(crate) iban: Option<String>,
    pub(crate) bic: Option<String>,
    pub(crate) email: Option<String>,
    pub(crate) vat_payer: bool,
    pub(crate) registration: String,
}

/// An invoice document.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainInvoice {
    pub(crate) number: String,
    pub(crate) client: String,
    pub(crate) due_on: Option<String>,
    /// `draft` or `scheduled` for unposted invoices; absent once issued.
    pub(crate) state: Option<String>,
    pub(crate) scheduled_for: Option<String>,
    pub(crate) lines: Vec<DomainInvoiceLine>,
}

/// An invoice line.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainInvoiceLine {
    pub(crate) description: String,
    pub(crate) quantity: String,
    pub(crate) unit: String,
    pub(crate) unit_price_minor: i64,
    /// A VAT code from the rule pack; the rate comes from the pack.
    pub(crate) vat_code: String,
}

/// A bank import.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainBankImport {
    pub(crate) file: String,
    pub(crate) format: String,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) opening_as_of: String,
    pub(crate) reported_closing_minor: i64,
    pub(crate) lines: Vec<DomainBankLine>,
}

/// A bank line.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainBankLine {
    pub(crate) id: String,
    pub(crate) date: String,
    pub(crate) counterparty: String,
    pub(crate) reference: String,
    pub(crate) amount_minor: i64,
    pub(crate) foreign: Option<DomainForeign>,
    pub(crate) status: String,
    pub(crate) matched_to: Option<String>,
    pub(crate) proposal_id: Option<String>,
    #[serde(default)]
    pub(crate) candidates: Vec<crate::dto::MatchCandidateDto>,
}

/// A foreign amount.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainForeign {
    pub(crate) amount_minor: i64,
    pub(crate) currency: String,
    pub(crate) rate: String,
}

/// An inbox item.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainProposal {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) detail: String,
    pub(crate) confidence: Option<String>,
    pub(crate) source_kind: String,
    pub(crate) source: String,
    pub(crate) bank_line_id: Option<String>,
    pub(crate) due_on: Option<String>,
    /// For deadlines: computed from the rule pack instead of stored.
    pub(crate) deadline: Option<DomainDeadline>,
    pub(crate) entry: Option<DomainEntry>,
    #[serde(default)]
    pub(crate) reasons: Vec<String>,
}

/// A deadline the core computes: `key` days after `period_end`, shifted.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainDeadline {
    pub(crate) key: String,
    pub(crate) period_end: String,
}

/// A proposed entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainEntry {
    pub(crate) date: String,
    pub(crate) memo: String,
    pub(crate) lines: Vec<DomainEntryLine>,
    #[serde(default)]
    pub(crate) settles: Vec<DomainSettle>,
    /// For a reverse-charge posting: the base and rate the VAT lines must equal.
    pub(crate) reverse_charge: Option<DomainVatCheck>,
    /// For a purchase with VAT: the gross and rate the split must equal.
    pub(crate) vat_split: Option<DomainVatCheck>,
}

/// A proposed line, signed (debit positive).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainEntryLine {
    pub(crate) account: String,
    pub(crate) amount_minor: i64,
    pub(crate) vat_code: Option<String>,
}

/// A proposed settlement.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainSettle {
    pub(crate) invoice: String,
    pub(crate) amount_minor: i64,
}

/// VAT figures the core re-derives with the engine.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DomainVatCheck {
    pub(crate) base_minor: Option<i64>,
    pub(crate) gross_minor: Option<i64>,
    pub(crate) vat_code: String,
}

/// Parses `demo-domain.json`.
pub(crate) fn demo_domain() -> Result<Domain, CoreError> {
    serde_json::from_str(DOMAIN).map_err(|e| CoreError::Demo(e.to_string()))
}
