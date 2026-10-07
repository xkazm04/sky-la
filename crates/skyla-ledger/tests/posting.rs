//! WP-05 acceptance: the posting engine and invariants I1–I4, I7, checked
//! through the Rust API and again through raw SQL that bypasses it.

use proptest::prelude::*;
use rusqlite::{Connection, params};
use skyla_ledger::{
    ChartSpec, EntryStatus, LedgerError, NewEntry, NewLine, SourceKind, add_analytic_account,
    apply_schema, create_draft, delete_draft, get_entry, list_accounts, open_period, post_entry,
    seed_chart, set_functional_currency,
};
use skyla_money::{Currency, Money, Rate};

const CZ_CHART: &str = include_str!("../../../rules/cz/chart.toml");

fn ledger() -> Connection {
    let conn = Connection::open_in_memory().expect("db");
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .expect("pragma");
    apply_schema(&conn).expect("schema");
    seed_chart(&conn, &ChartSpec::from_toml(CZ_CHART).expect("chart")).expect("seed");
    set_functional_currency(&conn, Currency::CZK).expect("currency");
    open_period(&conn, "2025-01-01", "2025-12-31").expect("2025");
    open_period(&conn, "2026-01-01", "2026-12-31").expect("2026");
    conn.execute_batch(
        "UPDATE period SET state = 'closing' WHERE starts_on = '2025-01-01';
         UPDATE period SET state = 'closed', closed_by = 'test', closed_at = '2026-01-10T00:00:00Z'
         WHERE starts_on = '2025-01-01';",
    )
    .expect("close 2025");
    conn
}

fn czk(minor: i64) -> Money {
    Money::new(minor, Currency::CZK)
}

fn entry(date: &str, source: SourceKind, lines: Vec<NewLine>) -> NewEntry {
    NewEntry {
        date: date.into(),
        source_kind: source,
        source_ref: None,
        memo: String::new(),
        created_by: "user".into(),
        lines,
    }
}

fn debit(account: &str, minor: i64) -> NewLine {
    NewLine::debit(account, czk(minor))
}

fn credit(account: &str, minor: i64) -> NewLine {
    NewLine::credit(account, czk(minor)).expect("credit")
}

fn rule(err: LedgerError) -> String {
    match err {
        LedgerError::Rule(message) => message,
        other => panic!("expected a database rule violation, got {other:?}"),
    }
}

fn raw_rule(conn: &Connection, sql: &str) -> String {
    rule(LedgerError::from(
        conn.execute_batch(sql)
            .expect_err("raw SQL should be rejected"),
    ))
}

#[test]
fn posts_the_northwind_receipt_and_the_aws_reverse_charge() {
    let conn = ledger();
    let receipt = create_draft(
        &conn,
        &entry(
            "2026-09-29",
            SourceKind::Bank,
            vec![debit("221", 8_470_000), credit("311", 8_470_000)],
        ),
    )
    .unwrap();
    // Bank entries come from deterministic matching; no approval needed.
    assert_eq!(post_entry(&conn, receipt, None).unwrap(), 1);

    // The reverse-charge posting from the design canvas: four lines, balanced at 14 905,51.
    let aws = create_draft(
        &conn,
        &entry(
            "2026-09-27",
            SourceKind::Rule,
            vec![
                debit("518", 1_231_860),
                credit("221", 1_231_860),
                debit("343", 258_691),
                credit("343", 258_691),
            ],
        ),
    )
    .unwrap();
    assert!(matches!(
        post_entry(&conn, aws, None),
        Err(LedgerError::NeedsApproval)
    ));
    assert!(matches!(
        post_entry(&conn, aws, Some("  ")),
        Err(LedgerError::NeedsApproval)
    ));
    assert_eq!(post_entry(&conn, aws, Some("user")).unwrap(), 2);

    let stored = get_entry(&conn, aws).unwrap();
    assert_eq!(stored.status, EntryStatus::Posted);
    assert_eq!(stored.approved_by.as_deref(), Some("user"));
    assert_eq!(stored.lines.len(), 4);
    assert_eq!(stored.lines[0].account, "518");
    assert!(matches!(
        post_entry(&conn, aws, Some("user")),
        Err(LedgerError::AlreadyPosted(_))
    ));
}

#[test]
fn foreign_currency_lines_carry_their_conversion() {
    let conn = ledger();
    let rate = Rate::new(25_140, 3);
    let eur = Money::new(49_000, Currency::EUR);
    let converted = eur
        .convert(rate, Currency::CZK, skyla_money::RoundingMode::HalfUp)
        .unwrap();
    let mut eur_line = NewLine::debit("518", eur);
    eur_line.conversion = Some((converted, rate));
    let id = create_draft(
        &conn,
        &entry(
            "2026-09-27",
            SourceKind::Manual,
            vec![eur_line.clone(), credit("221", 1_231_860)],
        ),
    )
    .unwrap();
    post_entry(&conn, id, None).unwrap();
    let stored = get_entry(&conn, id).unwrap();
    assert_eq!(stored.lines[0].amount, eur);
    assert_eq!(stored.lines[0].functional, czk(1_231_860));
    assert_eq!(stored.lines[0].fx_rate.as_deref(), Some("25.140"));

    let mut missing = NewLine::debit("518", eur);
    missing.conversion = None;
    let err = create_draft(
        &conn,
        &entry(
            "2026-09-27",
            SourceKind::Manual,
            vec![missing, credit("221", 1)],
        ),
    )
    .unwrap_err();
    assert!(matches!(err, LedgerError::InvalidLine { line: 1, .. }));
    let mut flipped = eur_line;
    flipped.conversion = Some((czk(-1_231_860), rate));
    assert!(matches!(
        create_draft(
            &conn,
            &entry("2026-09-27", SourceKind::Manual, vec![flipped])
        ),
        Err(LedgerError::InvalidLine { .. })
    ));
}

#[test]
fn rejects_each_kind_of_bad_entry_with_a_clear_error() {
    let conn = ledger();
    let unbalanced = create_draft(
        &conn,
        &entry(
            "2026-03-01",
            SourceKind::Manual,
            vec![debit("501", 1_000), credit("221", 999)],
        ),
    )
    .unwrap();
    match post_entry(&conn, unbalanced, None) {
        Err(LedgerError::Unbalanced { difference }) => assert_eq!(difference, czk(1)),
        other => panic!("expected Unbalanced, got {other:?}"),
    }
    let single = create_draft(
        &conn,
        &entry("2026-03-01", SourceKind::Manual, vec![debit("501", 1)]),
    )
    .unwrap();
    assert!(matches!(
        post_entry(&conn, single, None),
        Err(LedgerError::TooFewLines)
    ));

    let closed = create_draft(
        &conn,
        &entry(
            "2025-06-30",
            SourceKind::Manual,
            vec![debit("501", 5), credit("221", 5)],
        ),
    )
    .unwrap();
    assert!(matches!(
        post_entry(&conn, closed, None),
        Err(LedgerError::PeriodClosed(_))
    ));
    let no_period = create_draft(
        &conn,
        &entry(
            "2027-01-02",
            SourceKind::Manual,
            vec![debit("501", 5), credit("221", 5)],
        ),
    )
    .unwrap();
    assert!(matches!(
        post_entry(&conn, no_period, None),
        Err(LedgerError::NoPeriod(_))
    ));

    assert!(matches!(
        create_draft(&conn, &entry("2026-02-29", SourceKind::Manual, vec![])),
        Err(LedgerError::InvalidDate(_))
    ));
    assert!(matches!(
        create_draft(
            &conn,
            &entry("2026-03-01", SourceKind::Manual, vec![debit("999", 1)])
        ),
        Err(LedgerError::UnknownAccount(_))
    ));
    assert!(matches!(
        create_draft(
            &conn,
            &entry("2026-03-01", SourceKind::Manual, vec![debit("501", 0)])
        ),
        Err(LedgerError::InvalidLine { .. })
    ));

    add_analytic_account(&conn, "261", "001", "Card terminal", "Card terminal").unwrap();
    match create_draft(
        &conn,
        &entry("2026-03-01", SourceKind::Manual, vec![debit("261", 1)]),
    ) {
        Err(LedgerError::AccountNotPostable { code, reason }) => {
            assert_eq!((code.as_str(), reason), ("261", "it has sub-accounts"))
        }
        other => panic!("expected AccountNotPostable, got {other:?}"),
    }
    conn.execute_batch("UPDATE account SET active = 0 WHERE code = '231'")
        .unwrap();
    assert!(matches!(
        create_draft(
            &conn,
            &entry("2026-03-01", SourceKind::Manual, vec![debit("231", 1)])
        ),
        Err(LedgerError::AccountNotPostable {
            reason: "it is inactive",
            ..
        })
    ));

    // Nothing above reached the books.
    let posted: i64 = conn
        .query_row(
            "SELECT count(*) FROM journal_entry WHERE status = 'posted'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(posted, 0);
}

#[test]
fn posted_entries_are_frozen_even_against_raw_sql() {
    let conn = ledger();
    let id = create_draft(
        &conn,
        &entry(
            "2026-05-05",
            SourceKind::Manual,
            vec![debit("501", 2_500), credit("211", 2_500)],
        ),
    )
    .unwrap();
    post_entry(&conn, id, None).unwrap();

    assert!(
        raw_rule(
            &conn,
            "UPDATE posting SET amount_minor = 9, amount_func_minor = 9 WHERE line_no = 1"
        )
        .contains("immutable")
    );
    assert!(raw_rule(&conn, "DELETE FROM posting WHERE line_no = 1").contains("immutable"));
    assert!(raw_rule(&conn, "UPDATE journal_entry SET memo = 'edited'").contains("immutable"));
    assert!(raw_rule(&conn, "DELETE FROM journal_entry").contains("never deleted"));
    assert!(raw_rule(
        &conn,
        "INSERT INTO posting (entry_id, line_no, account_id, amount_minor, currency, amount_func_minor)
         SELECT 1, 3, id, 1, 'CZK', 1 FROM account WHERE code = '501'"
    )
    .contains("can't gain postings"));
    assert!(matches!(
        delete_draft(&conn, id),
        Err(LedgerError::AlreadyPosted(_))
    ));

    // The functional currency and the posted-to account's structure are now fixed too.
    assert!(
        rule(set_functional_currency(&conn, Currency::EUR).unwrap_err())
            .contains("can't change once anything is booked")
    );
    assert!(
        rule(add_analytic_account(&conn, "501", "001", "x", "x").unwrap_err())
            .contains("can't gain sub-accounts")
    );
}

#[test]
fn the_database_rejects_invalid_posts_that_bypass_rust() {
    let conn = ledger();
    let account = |code: &str| -> i64 {
        conn.query_row("SELECT id FROM account WHERE code = ?1", [code], |r| {
            r.get(0)
        })
        .unwrap()
    };
    let period: i64 = conn
        .query_row(
            "SELECT id FROM period WHERE starts_on = '2026-01-01'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let raw_draft = |uid: &str, source: &str, lines: &[(i64, i64)]| -> i64 {
        conn.execute("INSERT INTO journal_entry (uid, entry_date, source_kind, created_by) VALUES (?1, '2026-04-01', ?2, 'x')", params![uid, source]).unwrap();
        let id = conn.last_insert_rowid();
        for (n, (acct, amount)) in lines.iter().enumerate() {
            conn.execute(
                "INSERT INTO posting (entry_id, line_no, account_id, amount_minor, currency, amount_func_minor) VALUES (?1, ?2, ?3, ?4, 'CZK', ?4)",
                params![id, n as i64 + 1, acct, amount],
            )
            .unwrap();
        }
        id
    };
    let post_raw = |id: i64, seq: i64, approver: Option<&str>| -> rusqlite::Result<usize> {
        conn.execute(
            "UPDATE journal_entry SET status = 'posted', period_id = ?2, posted_seq = ?3, posted_at = 'now', approved_by = ?4, chain_hash = ?5 WHERE id = ?1",
            params![id, period, seq, approver, "0".repeat(64)],
        )
    };
    let reason =
        |r: rusqlite::Result<usize>| rule(LedgerError::from(r.expect_err("must be rejected")));

    let unbalanced = raw_draft(
        "u1",
        "manual",
        &[(account("501"), 100), (account("221"), -99)],
    );
    assert!(reason(post_raw(unbalanced, 1, None)).contains("must balance"));

    let advisor = raw_draft(
        "u2",
        "advisor",
        &[(account("501"), 100), (account("221"), -100)],
    );
    assert!(reason(post_raw(advisor, 1, None)).contains("human approver"));
    assert!(reason(post_raw(advisor, 7, Some("user"))).contains("gapless"));
    post_raw(advisor, 1, Some("user")).unwrap();

    let wrong_period = raw_draft("u3", "manual", &[(account("501"), 5), (account("221"), -5)]);
    conn.execute_batch("UPDATE journal_entry SET entry_date = '2025-04-01' WHERE uid = 'u3'")
        .unwrap();
    assert!(reason(post_raw(wrong_period, 2, None)).contains("no open period"));

    assert!(raw_rule(&conn, "INSERT INTO journal_entry (uid, entry_date, status, source_kind, created_by) VALUES ('u4', '2026-01-01', 'posted', 'manual', 'x')").contains("start as drafts"));
    assert!(raw_rule(
        &conn,
        &format!("INSERT INTO posting (entry_id, line_no, account_id, amount_minor, currency, amount_func_minor, fx_rate) VALUES ({unbalanced}, 9, {}, 1, 'CZK', 1, '1.0')", account("501"))
    )
    .contains("no FX rate"));
    assert!(raw_rule(
        &conn,
        &format!("INSERT INTO posting (entry_id, line_no, account_id, amount_minor, currency, amount_func_minor) VALUES ({unbalanced}, 9, {}, 1, 'EUR', 25)", account("501"))
    )
    .contains("needs its FX rate"));
}

/// The leaf accounts a fuzzer may post to.
fn postable(conn: &Connection) -> Vec<String> {
    list_accounts(conn)
        .expect("accounts")
        .into_iter()
        .filter(|a| a.is_leaf && a.active)
        .map(|a| a.code)
        .collect()
}

/// Builds lines that balance: random debits, then one closing credit.
fn balanced_lines(accounts: &[String], picks: &[(usize, i64)]) -> Vec<NewLine> {
    let mut lines: Vec<NewLine> = picks
        .iter()
        .map(|(i, minor)| debit(&accounts[i % accounts.len()], *minor))
        .collect();
    let total: i64 = picks.iter().map(|(_, m)| m).sum();
    lines.push(credit(&accounts[picks[0].0 % accounts.len()], total));
    lines
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    #[test]
    fn any_balanced_entry_posts(picks in prop::collection::vec((0_usize..1000, 1_i64..10_000_000_000), 1..7), day in 1_u32..28) {
        let conn = ledger();
        let accounts = postable(&conn);
        let id = create_draft(&conn, &entry(&format!("2026-06-{day:02}"), SourceKind::Manual, balanced_lines(&accounts, &picks))).unwrap();
        prop_assert_eq!(post_entry(&conn, id, None).unwrap(), 1);
    }

    #[test]
    fn any_unbalanced_entry_is_rejected_by_rust_and_by_sql(picks in prop::collection::vec((0_usize..1000, 1_i64..10_000_000_000), 1..7), off in prop_oneof![-1_000_i64..-1, 1_i64..1_000]) {
        let conn = ledger();
        let accounts = postable(&conn);
        let mut lines = balanced_lines(&accounts, &picks);
        let last = lines.len() - 1;
        lines[last].amount = czk(lines[last].amount.minor() + off);
        let id = create_draft(&conn, &entry("2026-06-15", SourceKind::Manual, lines)).unwrap();
        let rust_rejected = matches!(post_entry(&conn, id, None), Err(LedgerError::Unbalanced { .. }));
        prop_assert!(rust_rejected);
        let period: i64 = conn.query_row("SELECT id FROM period WHERE starts_on = '2026-01-01'", [], |r| r.get(0)).unwrap();
        let sql = conn.execute("UPDATE journal_entry SET status = 'posted', period_id = ?2, posted_seq = 1, posted_at = 'x' WHERE id = ?1", params![id, period]);
        prop_assert!(sql.is_err());
    }
}

/// A tiny deterministic generator so the bulk run is reproducible.
struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn ten_thousand_random_entries_leave_no_invariant_violated() {
    let conn = ledger();
    let accounts = postable(&conn);
    let mut rng = XorShift(0x5EED_5A1A_u64.wrapping_mul(2_654_435_761));
    let (mut posted, mut rejected) = (0_i64, 0_i64);
    conn.execute_batch("BEGIN").unwrap();
    for n in 0..10_000 {
        let picks: Vec<(usize, i64)> = (0..1 + rng.below(5))
            .map(|_| (rng.below(1000) as usize, 1 + rng.below(5_000_000) as i64))
            .collect();
        let mut lines = balanced_lines(&accounts, &picks);
        let corrupt = rng.below(5) == 0;
        if corrupt {
            let last = lines.len() - 1;
            lines[last].amount = czk(lines[last].amount.minor() - 1 - rng.below(100) as i64);
        }
        let date = format!("2026-{:02}-{:02}", 1 + rng.below(12), 1 + rng.below(28));
        let id = create_draft(&conn, &entry(&date, SourceKind::Manual, lines)).unwrap();
        match post_entry(&conn, id, None) {
            Ok(seq) => {
                assert!(!corrupt, "entry {n} was unbalanced but posted");
                posted += 1;
                assert_eq!(seq, posted);
            }
            Err(LedgerError::Unbalanced { .. }) => {
                assert!(corrupt, "entry {n} was balanced but rejected");
                rejected += 1;
            }
            Err(other) => panic!("entry {n}: unexpected {other:?}"),
        }
    }
    conn.execute_batch("COMMIT").unwrap();
    assert!(
        posted > 7_000 && rejected > 1_500,
        "posted {posted}, rejected {rejected}"
    );

    let total: i64 = conn
        .query_row("SELECT coalesce(sum(p.amount_func_minor), 0) FROM posting p JOIN journal_entry e ON e.id = p.entry_id WHERE e.status = 'posted'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 0, "the trial balance must sum to zero");
    let unbalanced: i64 = conn
        .query_row(
            "SELECT count(*) FROM (SELECT e.id FROM journal_entry e JOIN posting p ON p.entry_id = e.id WHERE e.status = 'posted' GROUP BY e.id HAVING sum(p.amount_func_minor) <> 0)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unbalanced, 0);
    let (min, max, count): (i64, i64, i64) = conn
        .query_row(
            "SELECT min(posted_seq), max(posted_seq), count(posted_seq) FROM journal_entry",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (min, max, count),
        (1, posted, posted),
        "posting sequence must be gapless"
    );
}

#[test]
fn an_account_deactivated_after_drafting_blocks_the_post() {
    let conn = ledger();
    let id = create_draft(
        &conn,
        &entry(
            "2026-02-02",
            SourceKind::Manual,
            vec![debit("512", 4_200), credit("211", 4_200)],
        ),
    )
    .unwrap();
    conn.execute_batch("UPDATE account SET active = 0 WHERE code = '512'")
        .unwrap();
    assert!(matches!(
        post_entry(&conn, id, None),
        Err(LedgerError::AccountNotPostable {
            reason: "it is inactive",
            ..
        })
    ));
    // And the database refuses it even when Rust is bypassed.
    let period: i64 = conn
        .query_row(
            "SELECT id FROM period WHERE starts_on = '2026-01-01'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let sql = conn.execute(
        "UPDATE journal_entry SET status = 'posted', period_id = ?2, posted_seq = 1, posted_at = 'x', chain_hash = ?3 WHERE id = ?1",
        params![id, period, "0".repeat(64)],
    );
    assert!(
        rule(LedgerError::from(sql.unwrap_err())).contains("active account without sub-accounts")
    );
}

#[test]
fn a_failed_operation_inside_a_caller_transaction_rolls_back_only_itself() {
    let conn = ledger();
    conn.execute_batch("BEGIN").unwrap();
    let kept = create_draft(
        &conn,
        &entry(
            "2026-01-05",
            SourceKind::Manual,
            vec![debit("501", 10), credit("211", 10)],
        ),
    )
    .unwrap();
    // The second draft's second line is invalid: that draft disappears entirely, the first stays.
    assert!(
        create_draft(
            &conn,
            &entry(
                "2026-01-05",
                SourceKind::Manual,
                vec![debit("501", 10), debit("999", 1)]
            )
        )
        .is_err()
    );
    conn.execute_batch("COMMIT").unwrap();
    let entries: i64 = conn
        .query_row("SELECT count(*) FROM journal_entry", [], |r| r.get(0))
        .unwrap();
    let lines: i64 = conn
        .query_row("SELECT count(*) FROM posting", [], |r| r.get(0))
        .unwrap();
    assert_eq!((entries, lines), (1, 2));
    assert_eq!(get_entry(&conn, kept).unwrap().lines.len(), 2);
}

#[test]
fn drafts_can_be_deleted() {
    let conn = ledger();
    let id = create_draft(
        &conn,
        &entry(
            "2026-01-10",
            SourceKind::Manual,
            vec![debit("501", 1), credit("211", 1)],
        ),
    )
    .unwrap();
    delete_draft(&conn, id).unwrap();
    assert!(matches!(
        get_entry(&conn, id),
        Err(LedgerError::EntryNotFound(_))
    ));
    let lines: i64 = conn
        .query_row("SELECT count(*) FROM posting", [], |r| r.get(0))
        .unwrap();
    assert_eq!(lines, 0);
}
