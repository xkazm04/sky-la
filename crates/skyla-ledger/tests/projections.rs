//! WP-07 acceptance: projections over the design-canvas golden journal
//! (`packages/fixtures/data/demo-ledger.json`).

use std::collections::HashMap;

use rusqlite::Connection;
use serde::Deserialize;
use skyla_ledger::{
    ChartSpec, Direction, LedgerError, NewEntry, NewLine, SourceKind, TaxTreatment, VatPart,
    VatRowRule, apply_schema, balance_sheet, cash_basis, create_draft, get_entry, link_settlement,
    open_period, post_entry, profit_and_loss, reverse_entry, seed_chart, set_functional_currency,
    trial_balance, vat_ledger, verify_chain,
};
use skyla_money::{Currency, Money, Rate};

const CZ_CHART: &str = include_str!("../../../rules/cz/chart.toml");
const GOLDEN: &str = include_str!("../../../packages/fixtures/data/demo-ledger.json");

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

fn czk(minor: i64) -> Money {
    Money::new(minor, Currency::CZK)
}

fn source(text: &str) -> SourceKind {
    match text {
        "manual" => SourceKind::Manual,
        "invoice" => SourceKind::Invoice,
        "bank" => SourceKind::Bank,
        "opening" => SourceKind::Opening,
        "reversal" => SourceKind::Reversal,
        other => panic!("unexpected source {other}"),
    }
}

/// The canvas entity's ledger, with the id of every golden entry by key.
fn golden() -> (Connection, HashMap<String, i64>) {
    let golden: Golden = serde_json::from_str(GOLDEN).expect("golden json");
    let conn = Connection::open_in_memory().expect("db");
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .expect("pragma");
    apply_schema(&conn).expect("schema");
    seed_chart(&conn, &ChartSpec::from_toml(CZ_CHART).expect("chart")).expect("seed");
    set_functional_currency(
        &conn,
        Currency::from_code(&golden.functional_currency).expect("ccy"),
    )
    .expect("currency");
    for (start, end) in &golden.periods {
        open_period(&conn, start, end).expect("period");
    }
    let mut ids = HashMap::new();
    for e in &golden.entries {
        let id = if let Some(original) = &e.reverses {
            let reversal = reverse_entry(&conn, ids[original], &e.date, "user", Some(&e.memo))
                .expect("reversal");
            // The golden lines document what the reversal must contain.
            let posted = get_entry(&conn, reversal.entry_id).expect("entry");
            let amounts: Vec<(String, i64)> = posted
                .lines
                .iter()
                .map(|l| (l.account.clone(), l.functional.minor()))
                .collect();
            let expected: Vec<(String, i64)> = e
                .lines
                .iter()
                .map(|l| (l.account.clone(), l.amount_minor))
                .collect();
            assert_eq!(amounts, expected, "reversal {}", e.key);
            reversal.entry_id
        } else {
            let lines = e
                .lines
                .iter()
                .map(|l| {
                    let currency = Currency::from_code(l.currency.as_deref().unwrap_or("CZK"))
                        .expect("line ccy");
                    NewLine {
                        account: l.account.clone(),
                        amount: Money::new(l.amount_minor, currency),
                        conversion: l.fx_rate.as_ref().map(|rate| {
                            (
                                czk(l.functional_minor.expect("functional amount")),
                                rate.parse::<Rate>().expect("rate"),
                            )
                        }),
                        vat_code: l.vat_code.clone(),
                        tax_treatment: None,
                        memo: l.memo.clone().unwrap_or_default(),
                    }
                })
                .collect();
            let id = create_draft(
                &conn,
                &NewEntry {
                    date: e.date.clone(),
                    source_kind: source(&e.source),
                    source_ref: e.reference.clone(),
                    memo: e.memo.clone(),
                    created_by: "user".into(),
                    lines,
                },
            )
            .expect("draft");
            for s in &e.settles {
                link_settlement(&conn, id, ids[&s.entry], czk(s.amount_minor)).expect("settle");
            }
            post_entry(&conn, id, None).expect("post");
            id
        };
        ids.insert(e.key.clone(), id);
    }
    (conn, ids)
}

fn kc(text: &str) -> Money {
    skyla_money::parse_amount_cs(text, Currency::CZK).expect("amount")
}

fn amounts(lines: &[skyla_ledger::StatementLine]) -> Vec<(&str, Money)> {
    lines.iter().map(|l| (l.code.as_str(), l.amount)).collect()
}

#[test]
fn the_canvas_q3_profit_and_loss_to_the_cent() {
    let (conn, _) = golden();
    assert!(verify_chain(&conn).unwrap().is_intact());

    let q3 = profit_and_loss(&conn, "2026-07-01", "2026-09-30").unwrap();
    assert_eq!(
        amounts(&q3.revenue),
        [("602", kc("455 200,00")), ("648", kc("1 300,00"))]
    );
    assert_eq!(
        amounts(&q3.expenses),
        [
            ("501", kc("41 300,00")),
            ("512", kc("6 200,00")),
            ("513", kc("3 400,00")),
            ("518", kc("120 800,00")),
            ("538", kc("1 200,00")),
            ("548", kc("2 400,00")),
            ("568", kc("850,00")),
        ]
    );
    // The canvas groups 538 · 568 · 549 as "Fees, bank charges, other".
    let fees: i64 = q3
        .expenses
        .iter()
        .filter(|l| ["538", "548", "568"].contains(&l.code.as_str()))
        .map(|l| l.amount.minor())
        .sum();
    assert_eq!(czk(fees), kc("4 450,00"));
    assert_eq!(q3.total_revenue, kc("456 500,00"));
    assert_eq!(q3.total_expenses, kc("176 150,00"));
    assert_eq!(q3.profit, kc("280 350,00"));
    assert_eq!(q3.revenue[0].name_en, "Sales of services");

    // The comparison column.
    let q2 = profit_and_loss(&conn, "2026-04-01", "2026-06-30").unwrap();
    assert_eq!(q2.total_revenue, kc("391 000,00"));
    assert_eq!(q2.total_expenses, kc("112 220,00"));
    assert_eq!(q2.profit, kc("278 780,00"));
}

#[test]
fn the_trial_balance_and_balance_sheet_agree_with_the_journal() {
    let (conn, _) = golden();
    let tb = trial_balance(&conn, None, "2026-09-30").unwrap();
    assert!(tb.balances());
    assert!(
        tb.rows
            .iter()
            .all(|r| r.debit.minor() - r.credit.minor() == r.balance.minor())
    );
    // The reverse charge books output and input VAT on 343, which cancel.
    let vat = tb.rows.iter().find(|r| r.code == "343").unwrap();
    assert!(vat.debit.minor() > 0 && vat.credit.minor() > 0);

    let bs = balance_sheet(&conn, "2026-09-30").unwrap();
    assert!(bs.balances(), "{bs:#?}");
    let half_year = profit_and_loss(&conn, "2026-04-01", "2026-09-30").unwrap();
    assert_eq!(bs.unclosed_profit, half_year.profit);
    assert_eq!(bs.unclosed_profit, kc("559 130,00"));
    // Only Studio Brno's August invoice is still partly open.
    let receivables = bs.assets.iter().find(|l| l.code == "311").unwrap();
    assert_eq!(receivables.amount, kc("53 092,00"));
    let equity = bs.equity.iter().find(|l| l.code == "491").unwrap();
    assert_eq!(equity.amount, kc("195 000,00"));

    // A movement-only trial balance for Q3 balances too.
    assert!(
        trial_balance(&conn, Some("2026-07-01"), "2026-09-30")
            .unwrap()
            .balances()
    );
    assert!(matches!(
        trial_balance(&conn, Some("2026-10-01"), "2026-09-30"),
        Err(LedgerError::InvalidEntry(_))
    ));
    assert!(matches!(
        balance_sheet(&conn, "2026-02-30"),
        Err(LedgerError::InvalidDate(_))
    ));
}

#[test]
fn cash_basis_income_for_the_northwind_receipt_is_the_base() {
    let (conn, ids) = golden();
    let day = cash_basis(&conn, "2026-09-29", "2026-09-29").unwrap();
    assert_eq!(day.lines.len(), 1);
    let line = &day.lines[0];
    assert_eq!(line.amount, kc("70 000,00"));
    assert_eq!(
        (line.direction, line.tax_treatment),
        (Direction::Income, TaxTreatment::Taxable)
    );
    assert_eq!(line.settled_entry_id, Some(ids["inv-2026-114"]));
    assert_eq!(line.cash_entry_id, ids["rcpt-114"]);
    assert_eq!(line.account, "602");
    assert_eq!(day.taxable_income, kc("70 000,00"));
}

#[test]
fn the_cash_basis_follows_money_not_invoices() {
    let (conn, ids) = golden();
    let q3 = cash_basis(&conn, "2026-07-01", "2026-09-30").unwrap();
    // Received in Q3: Q2's Studio Brno invoice (121 000), Northwind July
    // (140 000), Acme August (160 000), Northwind 2026-114 (70 000), part of
    // Studio Brno August (50 000 of 103 092 → 41 322,31 of the 85 200 base),
    // and the insurance refund (1 300). Acme's Q2 invoice was paid in Q2.
    assert_eq!(q3.taxable_income, kc("533 622,31"));
    let partial = q3
        .lines
        .iter()
        .find(|l| l.cash_entry_id == ids["rcpt-102a"])
        .unwrap();
    assert_eq!(partial.amount, kc("41 322,31"));
    // Entertainment is paid but not deductible.
    let non_deductible = q3
        .totals
        .iter()
        .find(|t| t.tax_treatment == TaxTreatment::NonDeductible)
        .unwrap();
    assert_eq!(non_deductible.amount, kc("3 400,00"));
    // Paid in Q3: Pixelfarm's Q2 invoice (15 500), Kvasnička Q3 (46 000) and
    // Horák Q3 (38 500), bases only; direct 501 (41 300), 512 (6 200), 518
    // (14 700) and fees (4 450). Pixelfarm's Q3 invoice is unpaid.
    assert_eq!(q3.deductible_expenses, kc("166 650,00"));
    // The duplicate Figma payment and its reversal cancel out.
    let figma: i64 = q3
        .lines
        .iter()
        .filter(|l| l.date.as_str() >= "2026-09-04" && l.date.as_str() <= "2026-09-05")
        .map(|l| l.amount.minor())
        .sum();
    assert_eq!(figma, 0);

    // Paying the rest in Q4 recognises exactly the remaining base.
    let rest = create_draft(
        &conn,
        &NewEntry {
            date: "2026-10-12".into(),
            source_kind: SourceKind::Bank,
            source_ref: None,
            memo: "Studio Brno (doplatek)".into(),
            created_by: "user".into(),
            lines: vec![
                NewLine::debit("221", czk(5_309_200)),
                NewLine::credit("311", czk(5_309_200)).unwrap(),
            ],
        },
    )
    .unwrap();
    link_settlement(&conn, rest, ids["inv-2026-102"], czk(5_309_200)).unwrap();
    post_entry(&conn, rest, None).unwrap();
    let q4 = cash_basis(&conn, "2026-10-01", "2026-12-31").unwrap();
    assert_eq!(
        q4.taxable_income.minor() + partial.amount.minor(),
        kc("85 200,00").minor()
    );
}

#[test]
fn settlement_links_follow_the_ledger_rules() {
    let (conn, ids) = golden();
    let draft = create_draft(
        &conn,
        &NewEntry {
            date: "2026-10-02".into(),
            source_kind: SourceKind::Bank,
            source_ref: None,
            memo: String::new(),
            created_by: "user".into(),
            lines: vec![
                NewLine::debit("221", czk(100)),
                NewLine::credit("311", czk(100)).unwrap(),
            ],
        },
    )
    .unwrap();
    // Over-settling: Studio Brno has 53 092 left.
    let over = link_settlement(&conn, draft, ids["inv-2026-102"], kc("53 092,01")).unwrap_err();
    assert!(matches!(over, LedgerError::Rule(m) if m.contains("exceed")));
    assert!(matches!(
        link_settlement(&conn, draft, ids["inv-2026-102"], czk(-1)),
        Err(LedgerError::InvalidEntry(_))
    ));
    assert!(matches!(
        link_settlement(&conn, ids["rcpt-114"], ids["inv-2026-102"], czk(1)),
        Err(LedgerError::AlreadyPosted(_))
    ));
    assert!(matches!(
        link_settlement(&conn, draft, draft, czk(1)),
        Err(LedgerError::NotPosted(_))
    ));
    let frozen = conn.execute_batch(&format!(
        "DELETE FROM settlement WHERE cash_entry_id = {}",
        ids["rcpt-114"]
    ));
    assert!(
        matches!(LedgerError::from(frozen.unwrap_err()), LedgerError::Rule(m) if m.contains("immutable"))
    );

    // Reversing the Northwind receipt un-recognises the income on its date.
    reverse_entry(&conn, ids["rcpt-114"], "2026-10-03", "user", None).unwrap();
    let day = cash_basis(&conn, "2026-10-03", "2026-10-03").unwrap();
    assert_eq!(day.taxable_income, kc("-70 000,00"));
    // And the invoice can be settled again.
    link_settlement(&conn, draft, ids["inv-2026-114"], czk(100)).unwrap();
    assert!(verify_chain(&conn).unwrap().is_intact());
}

/// CZ DPH form rows used here: 1 (domestic supply, 21 %), 5 (services received
/// from another member state, reverse charge, output side), 40 (domestic
/// input, 21 %), 43 (input on row 5's reverse charge). The real mapping comes
/// from the rule pack in WP-20.
fn cz_rows() -> Vec<VatRowRule> {
    let rule = |code: &str, row: &str, part, credit_positive| VatRowRule {
        vat_code: code.into(),
        row: row.into(),
        part,
        credit_positive,
    };
    vec![
        rule("OUT21", "1", VatPart::Base, true),
        rule("OUT21", "1", VatPart::Tax, true),
        rule("RC21S", "5", VatPart::Base, false),
        rule("RC21S", "5", VatPart::TaxCredit, true),
        rule("IN21", "40", VatPart::Base, false),
        rule("IN21", "40", VatPart::Tax, false),
        rule("RC21S", "43", VatPart::Base, false),
        rule("RC21S", "43", VatPart::TaxDebit, false),
    ]
}

#[test]
fn the_vat_ledger_by_form_row_for_september() {
    let (conn, _) = golden();
    let vat = vat_ledger(&conn, "2026-09-01", "2026-09-30", &["343"], &cz_rows()).unwrap();
    let rows: Vec<(&str, Money, Money)> = vat
        .rows
        .iter()
        .map(|r| (r.row.as_str(), r.base, r.tax))
        .collect();
    assert_eq!(
        rows,
        [
            ("1", kc("70 000,00"), kc("14 700,00")),
            ("5", kc("12 318,60"), kc("2 586,91")),
            ("40", kc("24 083,00"), kc("5 057,43")),
            ("43", kc("12 318,60"), kc("2 586,91")),
        ]
    );
    assert!(vat.unmapped.is_empty());
    let rc = vat.codes.iter().find(|c| c.vat_code == "RC21S").unwrap();
    assert_eq!(rc.tax_debit.minor() + rc.tax_credit.minor(), 0);

    let partial = vat_ledger(&conn, "2026-09-01", "2026-09-30", &["343"], &cz_rows()[..2]).unwrap();
    assert_eq!(partial.unmapped, ["IN21", "RC21S"]);
}

#[test]
fn every_report_stamps_a_reproducible_input_snapshot() {
    let (conn, _) = golden();
    let a = profit_and_loss(&conn, "2026-07-01", "2026-09-30")
        .unwrap()
        .snapshot;
    let b = profit_and_loss(&conn, "2026-07-01", "2026-09-30")
        .unwrap()
        .snapshot;
    assert_eq!(a, b);
    assert_eq!(a.hash.len(), 64);
    assert!(a.entries > 0 && a.last_posted_seq.is_some());
    assert_ne!(
        a.hash,
        profit_and_loss(&conn, "2026-07-01", "2026-09-29")
            .unwrap()
            .snapshot
            .hash
    );
    assert_ne!(
        a.hash,
        trial_balance(&conn, Some("2026-07-01"), "2026-09-30")
            .unwrap()
            .snapshot
            .hash
    );

    let q4 = |conn: &Connection, date: &str| {
        let id = create_draft(
            conn,
            &NewEntry {
                date: date.into(),
                source_kind: SourceKind::Manual,
                source_ref: None,
                memo: String::new(),
                created_by: "user".into(),
                lines: vec![
                    NewLine::debit("568", czk(100)),
                    NewLine::credit("221", czk(100)).unwrap(),
                ],
            },
        )
        .unwrap();
        post_entry(conn, id, None).unwrap();
    };
    // An entry outside the range leaves the Q3 snapshot alone; one inside changes it.
    q4(&conn, "2026-10-15");
    assert_eq!(
        profit_and_loss(&conn, "2026-07-01", "2026-09-30")
            .unwrap()
            .snapshot,
        a
    );
    q4(&conn, "2026-09-30");
    let c = profit_and_loss(&conn, "2026-07-01", "2026-09-30")
        .unwrap()
        .snapshot;
    assert_ne!(c.hash, a.hash);
    assert_eq!(c.entries, a.entries + 1);
}

/// WP-07 bench: a trial balance over 100 000 posted entries in under 200 ms.
/// Run with `just bench` (release build); in debug builds it only reports.
#[test]
#[ignore = "bench: run with `just bench`"]
fn trial_balance_over_100k_entries_is_fast() {
    let conn = Connection::open_in_memory().expect("db");
    apply_schema(&conn).expect("schema");
    seed_chart(&conn, &ChartSpec::from_toml(CZ_CHART).expect("chart")).expect("seed");
    set_functional_currency(&conn, Currency::CZK).expect("currency");
    open_period(&conn, "2026-01-01", "2026-12-31").expect("period");
    let accounts = [
        "501", "512", "518", "538", "568", "602", "311", "321", "343", "211",
    ];
    conn.execute_batch("BEGIN").expect("begin");
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    for i in 0..100_000_u32 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let a = accounts[(state % 10) as usize];
        let b = accounts[((state >> 8) % 10) as usize];
        let b = if a == b { "221" } else { b };
        let minor = (state >> 16) as i64 % 1_000_000 + 1;
        let id = create_draft(
            &conn,
            &NewEntry {
                date: format!("2026-{:02}-{:02}", i % 12 + 1, i % 28 + 1),
                source_kind: SourceKind::Manual,
                source_ref: None,
                memo: String::new(),
                created_by: "bench".into(),
                lines: vec![
                    NewLine::debit(a, czk(minor)),
                    NewLine::credit(b, czk(minor)).expect("credit"),
                ],
            },
        )
        .expect("draft");
        post_entry(&conn, id, None).expect("post");
    }
    conn.execute_batch("COMMIT").expect("commit");

    let mut best = std::time::Duration::MAX;
    for _ in 0..5 {
        let start = std::time::Instant::now();
        let tb = trial_balance(&conn, None, "2026-12-31").expect("tb");
        best = best.min(start.elapsed());
        assert!(tb.balances());
        assert_eq!(tb.snapshot.entries, 100_000);
    }
    println!("trial balance over 100 000 entries: best of 5 = {best:?}");
    if !cfg!(debug_assertions) {
        assert!(best < std::time::Duration::from_millis(200), "{best:?}");
    }
}
