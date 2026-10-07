//! WP-04 acceptance: the Czech chart loads, validates and seeds; the database
//! enforces account structure and periods on its own.

use rusqlite::Connection;
use skyla_ledger::{
    AccountKind, ChartSpec, Direction, LedgerError, NormalSide, TaxTreatment, add_analytic_account,
    apply_schema, list_accounts, list_categories, open_period, seed_chart,
};

const CZ_CHART: &str = include_str!("../../../rules/cz/chart.toml");

fn seeded() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .expect("pragma");
    apply_schema(&conn).expect("schema");
    seed_chart(&conn, &ChartSpec::from_toml(CZ_CHART).expect("CZ chart")).expect("seed");
    conn
}

fn rule(err: LedgerError) -> String {
    match err {
        LedgerError::Rule(message) => message,
        other => panic!("expected a ledger rule violation, got {other:?}"),
    }
}

#[test]
fn the_czech_chart_is_valid_and_complete() {
    let chart = ChartSpec::from_toml(CZ_CHART).unwrap();
    assert_eq!(chart.id, "cz-chart");
    assert!(chart.citation.contains("500/2002"));
    for code in [
        "211", "221", "311", "321", "343", "491", "501", "518", "602", "701", "702", "710",
    ] {
        assert!(
            chart.accounts.iter().any(|a| a.code == code),
            "missing account {code}"
        );
    }
    // Every category lands on an account of the matching kind.
    for category in &chart.categories {
        let account = chart
            .accounts
            .iter()
            .find(|a| a.code == category.account)
            .unwrap();
        let expected = match category.direction {
            Direction::Income => AccountKind::Revenue,
            Direction::Expense => AccountKind::Expense,
            Direction::Owner => AccountKind::Equity,
        };
        assert_eq!(account.kind, expected, "category {}", category.key);
    }
}

#[test]
fn the_seeded_chart_matches_its_snapshot() {
    let conn = seeded();
    let accounts: Vec<String> = list_accounts(&conn)
        .unwrap()
        .iter()
        .map(|a| {
            format!(
                "{:<8} {:<9} {:<6} {}{}{} {}",
                a.code,
                format!("{:?}", a.kind).to_lowercase(),
                format!("{:?}", a.normal_side).to_lowercase(),
                if a.contra { "contra " } else { "" },
                if a.cash { "cash " } else { "" },
                if a.is_leaf { "leaf" } else { "group" },
                a.name_en
            )
        })
        .collect();
    let categories: Vec<String> = list_categories(&conn)
        .unwrap()
        .iter()
        .map(|c| {
            format!(
                "{:<32} → {:<4} {:?}",
                c.key, c.account_code, c.tax_treatment
            )
        })
        .collect();
    insta::assert_snapshot!("cz_chart_accounts", accounts.join("\n"));
    insta::assert_snapshot!("cz_chart_categories", categories.join("\n"));
}

#[test]
fn validation_reports_every_problem_at_once() {
    let broken = r#"
id = "x"
version = "1"
citation = "test"

[[account]]
code = "602"
name_cs = "Tržby"
name_en = "Sales"
kind = "revenue"
normal_side = "debit"

[[account]]
code = "602"
name_cs = "Duplicate"
name_en = "Duplicate"
kind = "revenue"
normal_side = "credit"

[[account]]
code = "999.1"
name_cs = "Orphan"
name_en = "Orphan"
kind = "asset"
normal_side = "debit"

[[account]]
code = "42"
name_cs = "Short"
name_en = "Short"
kind = "asset"
normal_side = "debit"

[[category]]
key = "sales"
name_cs = "Prodej"
name_en = "Sales"
direction = "expense"
account = "602"
tax_treatment = "taxable"

[[category]]
key = "ghost"
name_cs = "Nic"
name_en = "Nothing"
direction = "income"
account = "604"
tax_treatment = "taxable"
"#;
    let Err(LedgerError::InvalidChart(problems)) = ChartSpec::from_toml(broken) else {
        panic!("expected InvalidChart");
    };
    let all = problems.join("\n");
    for expected in [
        "602: a revenue account is credit-normal",
        "602: duplicate code",
        "999.1: parent 999 is missing",
        "\"42\": code must be three digits",
        "sales: a expense category can't map to revenue account 602",
        "sales: tax treatment taxable doesn't fit a expense category",
        "ghost: account 604 does not exist",
    ] {
        assert!(
            all.contains(expected),
            "missing problem {expected:?} in:\n{all}"
        );
    }
    assert!(matches!(
        ChartSpec::from_toml("not = [valid"),
        Err(LedgerError::ChartFormat(_))
    ));
    assert!(matches!(
        ChartSpec::from_toml(
            "id = \"x\"\nversion = \"1\"\ncitation = \"c\"\nsurprise = 1\naccount = []"
        ),
        Err(LedgerError::ChartFormat(_))
    ));
}

#[test]
fn an_analytic_account_inherits_its_parent_and_closes_it_to_postings() {
    let conn = seeded();
    let csob = add_analytic_account(&conn, "221", "001", "ČSOB Business", "ČSOB Business").unwrap();
    assert_eq!(
        (csob.kind, csob.normal_side, csob.is_leaf),
        (AccountKind::Asset, NormalSide::Debit, true)
    );
    assert_eq!(csob.parent_code.as_deref(), Some("221"));
    let bank = list_accounts(&conn)
        .unwrap()
        .into_iter()
        .find(|a| a.code == "221")
        .unwrap();
    assert!(!bank.is_leaf);

    // A category posts to 518, so it can't gain sub-accounts until the category moves.
    let err = add_analytic_account(&conn, "518", "100", "Hosting", "Hosting").unwrap_err();
    assert!(rule(err).contains("a category posts to this account"));
    assert!(matches!(
        add_analytic_account(&conn, "999", "1", "X", "X"),
        Err(LedgerError::UnknownAccount(_))
    ));
}

#[test]
fn the_database_enforces_account_structure_even_for_raw_sql() {
    let conn = seeded();
    let attempt = |sql: &str| rule(LedgerError::from(conn.execute_batch(sql).unwrap_err()));

    assert!(attempt("INSERT INTO account (code, name_cs, name_en, kind, normal_side) VALUES ('22', 'x', 'x', 'asset', 'debit')").contains("exactly three digits"));
    assert!(
        attempt(
            "INSERT INTO account (code, name_cs, name_en, kind, normal_side, parent_id, cash)
         SELECT '221.001', 'x', 'x', 'liability', 'credit', id, 1 FROM account WHERE code = '221'"
        )
        .contains("parent's kind and normal side")
    );
    assert!(
        attempt(
            "INSERT INTO account (code, name_cs, name_en, kind, normal_side, parent_id, cash)
         SELECT '311.001', 'x', 'x', 'asset', 'debit', id, 1 FROM account WHERE code = '221'"
        )
        .contains("parent's code, a dot and a suffix")
    );
    // WP-07: cash accounts are assets, sub-accounts share the flag, and it never changes.
    assert!(
        attempt(
            "INSERT INTO account (code, name_cs, name_en, kind, normal_side, parent_id)
         SELECT '221.001', 'x', 'x', 'asset', 'debit', id FROM account WHERE code = '221'"
        )
        .contains("parent's cash flag")
    );
    assert!(
        attempt("INSERT INTO account (code, name_cs, name_en, kind, normal_side, cash) VALUES ('232', 'x', 'x', 'liability', 'credit', 1)")
            .contains("only asset accounts can be cash")
    );
    assert!(
        attempt("UPDATE account SET cash = 0 WHERE code = '221'")
            .contains("cash flag can't change")
    );
    assert!(
        attempt("UPDATE account SET kind = 'expense' WHERE code = '221'").contains("can't change")
    );
    assert!(attempt("DELETE FROM account WHERE code = '221'").contains("never deleted"));
    // Deactivating is allowed.
    conn.execute_batch("UPDATE account SET active = 0 WHERE code = '231'")
        .unwrap();
    assert!(
        !list_accounts(&conn)
            .unwrap()
            .iter()
            .find(|a| a.code == "231")
            .unwrap()
            .active
    );
}

#[test]
fn periods_are_valid_dates_and_never_overlap() {
    let conn = seeded();
    open_period(&conn, "2026-01-01", "2026-03-31").unwrap();
    open_period(&conn, "2026-04-01", "2026-06-30").unwrap();
    assert!(
        rule(open_period(&conn, "2026-03-15", "2026-04-15").unwrap_err())
            .contains("must not overlap")
    );
    assert!(
        rule(open_period(&conn, "2025-12-31", "2027-01-01").unwrap_err())
            .contains("must not overlap")
    );
    // Invalid or non-canonical dates and reversed ranges fail the CHECK constraints.
    // (These ranges don't overlap the periods above, so they reach the CHECKs.)
    for (start, end) in [
        ("2027-02-30", "2027-07-31"),
        ("2027-7-1", "2027-07-31"),
        ("2027-09-30", "2027-07-01"),
        ("nonsense", "2027-12-31"),
    ] {
        assert!(
            matches!(open_period(&conn, start, end), Err(LedgerError::Sql(_))),
            "{start}..{end} should be rejected"
        );
    }
    let err = LedgerError::from(
        conn.execute_batch("UPDATE period SET ends_on = '2026-04-30' WHERE id = 1")
            .unwrap_err(),
    );
    assert!(rule(err).contains("dates can't change"));
}

#[test]
fn tax_treatments_default_sensibly() {
    let conn = seeded();
    let categories = list_categories(&conn).unwrap();
    let treatment = |key: &str| {
        categories
            .iter()
            .find(|c| c.key == key)
            .unwrap()
            .tax_treatment
    };
    assert_eq!(
        treatment("client-entertainment"),
        TaxTreatment::NonDeductible
    );
    assert_eq!(
        treatment("software-subscriptions"),
        TaxTreatment::Deductible
    );
    assert_eq!(treatment("owner-withdrawal"), TaxTreatment::NotTaxRelevant);
    assert_eq!(treatment("services-sold"), TaxTreatment::Taxable);
}
