//! WP-06 acceptance: periods (open → closing → closed), reversals, close
//! checks and the journal hash chain.

use rusqlite::{Connection, params};
use skyla_ledger::{
    ChainBreakKind, ChartSpec, CloseCheck, EntryStatus, LedgerError, NewEntry, NewLine, Period,
    PeriodState, STANDARD_CHECKS, SourceKind, apply_schema, begin_close, close_period,
    create_draft, delete_draft, get_entry, get_period, list_periods, open_period, post_entry,
    reopen_period, reverse_entry, run_close_checks, seed_chart, set_functional_currency,
    verify_chain,
};
use skyla_money::{Currency, Money, Rate};

const CZ_CHART: &str = include_str!("../../../rules/cz/chart.toml");

struct Ledger {
    conn: Connection,
    q3: i64,
    q4: i64,
}

fn ledger() -> Ledger {
    let conn = Connection::open_in_memory().expect("db");
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .expect("pragma");
    apply_schema(&conn).expect("schema");
    seed_chart(&conn, &ChartSpec::from_toml(CZ_CHART).expect("chart")).expect("seed");
    set_functional_currency(&conn, Currency::CZK).expect("currency");
    let q3 = open_period(&conn, "2026-07-01", "2026-09-30").expect("q3");
    let q4 = open_period(&conn, "2026-10-01", "2026-12-31").expect("q4");
    Ledger { conn, q3, q4 }
}

fn czk(minor: i64) -> Money {
    Money::new(minor, Currency::CZK)
}

fn post(conn: &Connection, date: &str, memo: &str, lines: Vec<NewLine>) -> i64 {
    let id = create_draft(
        conn,
        &NewEntry {
            date: date.into(),
            source_kind: SourceKind::Manual,
            source_ref: None,
            memo: memo.into(),
            created_by: "user".into(),
            lines,
        },
    )
    .expect("draft");
    post_entry(conn, id, None).expect("post");
    id
}

fn pair(debit: &str, credit: &str, minor: i64) -> Vec<NewLine> {
    vec![
        NewLine::debit(debit, czk(minor)),
        NewLine::credit(credit, czk(minor)).expect("credit"),
    ]
}

/// Posted functional-currency balance of an account (debit positive).
fn balance(conn: &Connection, code: &str) -> i64 {
    conn.query_row(
        "SELECT coalesce(sum(p.amount_func_minor), 0) FROM posting p
         JOIN account a ON a.id = p.account_id JOIN journal_entry e ON e.id = p.entry_id
         WHERE a.code = ?1 AND e.status = 'posted'",
        [code],
        |r| r.get(0),
    )
    .expect("balance")
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

// ---------------------------------------------------------------- reversals

#[test]
fn a_reversal_leaves_every_balance_at_zero() {
    let Ledger { conn, .. } = ledger();
    // The Northwind invoice (base 70 000 + VAT 14 700) and an EUR expense.
    let invoice = post(
        &conn,
        "2026-09-15",
        "Faktura 2026-114 Northwind",
        vec![
            NewLine::debit("311", czk(8_470_000)),
            NewLine::credit("602", czk(7_000_000)).unwrap(),
            NewLine::credit("343", czk(1_470_000)).unwrap(),
        ],
    );
    let eur = post(
        &conn,
        "2026-09-20",
        "AWS",
        vec![
            NewLine {
                conversion: Some((czk(1_231_860), "25.14".parse::<Rate>().unwrap())),
                ..NewLine::debit("518", Money::new(49_000, Currency::EUR))
            },
            NewLine::credit("221", czk(1_231_860)).unwrap(),
        ],
    );
    assert_eq!(balance(&conn, "311"), 8_470_000);

    let r1 = reverse_entry(&conn, invoice, "2026-10-02", "user", None).unwrap();
    let r2 = reverse_entry(&conn, eur, "2026-09-20", "user", Some("wrong card")).unwrap();
    for code in ["311", "602", "343", "518", "221"] {
        assert_eq!(balance(&conn, code), 0, "account {code}");
    }
    // The foreign line came back in its own currency too.
    let foreign: i64 = conn
        .query_row(
            "SELECT sum(amount_minor) FROM posting WHERE currency = 'EUR'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(foreign, 0);

    let reversal = get_entry(&conn, r1.entry_id).unwrap();
    assert_eq!(reversal.status, EntryStatus::Posted);
    assert_eq!(reversal.source_kind, SourceKind::Reversal);
    assert_eq!(reversal.reverses_id, Some(invoice));
    assert_eq!(reversal.posted_seq, Some(r1.posted_seq));
    assert_eq!(
        reversal.memo,
        "Reversal of entry 1: Faktura 2026-114 Northwind"
    );
    assert_eq!(reversal.lines[2].amount, czk(1_470_000));
    assert_eq!(get_entry(&conn, r2.entry_id).unwrap().memo, "wrong card");
    assert!(verify_chain(&conn).unwrap().is_intact());
}

#[test]
fn reversals_are_refused_when_they_make_no_sense() {
    let Ledger { conn, q3, .. } = ledger();
    let posted = post(&conn, "2026-08-01", "rent", pair("518", "221", 1_500_000));
    let draft = create_draft(
        &conn,
        &NewEntry {
            date: "2026-08-02".into(),
            source_kind: SourceKind::Manual,
            source_ref: None,
            memo: String::new(),
            created_by: "user".into(),
            lines: pair("501", "221", 100),
        },
    )
    .unwrap();

    assert!(matches!(
        reverse_entry(&conn, draft, "2026-08-02", "user", None),
        Err(LedgerError::NotPosted(_))
    ));
    assert!(matches!(
        reverse_entry(&conn, posted, "2026-07-31", "user", None),
        Err(LedgerError::InvalidEntry(m)) if m.contains("before the entry it reverses")
    ));
    assert!(matches!(
        reverse_entry(&conn, posted, "2026-13-01", "user", None),
        Err(LedgerError::InvalidDate(_))
    ));
    assert!(matches!(
        create_draft(
            &conn,
            &NewEntry {
                date: "2026-08-03".into(),
                source_kind: SourceKind::Reversal,
                source_ref: None,
                memo: String::new(),
                created_by: "user".into(),
                lines: pair("221", "518", 1_500_000),
            }
        ),
        Err(LedgerError::InvalidEntry(_))
    ));
    reverse_entry(&conn, posted, "2026-08-05", "user", None).unwrap();
    assert!(matches!(
        reverse_entry(&conn, posted, "2026-08-06", "user", None),
        Err(LedgerError::AlreadyReversed(_))
    ));

    // Once Q3 is closed, the correction goes into Q4.
    delete_draft(&conn, draft).unwrap();
    let late = post(&conn, "2026-09-30", "late fee", pair("545", "221", 50_000));
    begin_close(&conn, q3).unwrap();
    close_period(&conn, q3, STANDARD_CHECKS, "user").unwrap();
    assert!(matches!(
        reverse_entry(&conn, late, "2026-09-30", "user", None),
        Err(LedgerError::PeriodClosed(_))
    ));
    // The failed attempt left nothing behind.
    let drafts: i64 = conn
        .query_row(
            "SELECT count(*) FROM journal_entry WHERE status = 'draft'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(drafts, 0);
    reverse_entry(&conn, late, "2026-10-01", "user", None).unwrap();
    assert_eq!(balance(&conn, "545"), 0);
}

#[test]
fn the_database_enforces_reversal_rules_against_raw_sql() {
    let Ledger { conn, q3, .. } = ledger();
    let original = post(&conn, "2026-08-01", "x", pair("501", "221", 1_000));
    let account = |code: &str| -> i64 {
        conn.query_row("SELECT id FROM account WHERE code = ?1", [code], |r| {
            r.get(0)
        })
        .unwrap()
    };
    assert!(raw_rule(&conn, "INSERT INTO journal_entry (uid, entry_date, source_kind, created_by) VALUES ('r0', '2026-08-02', 'reversal', 'x')")
        .contains("references the entry it reverses"));
    assert!(raw_rule(&conn, &format!("INSERT INTO journal_entry (uid, entry_date, source_kind, created_by, reverses_id) VALUES ('r0', '2026-08-02', 'manual', 'x', {original})"))
        .contains("only a reversal does"));

    // A "reversal" that doesn't mirror the original can't post.
    conn.execute(
        "INSERT INTO journal_entry (uid, entry_date, source_kind, created_by, reverses_id) VALUES ('r1', '2026-08-02', 'reversal', 'x', ?1)",
        [original],
    )
    .unwrap();
    let rid = conn.last_insert_rowid();
    for (n, (code, amount)) in [("501", -900), ("221", 900)].iter().enumerate() {
        conn.execute(
            "INSERT INTO posting (entry_id, line_no, account_id, amount_minor, currency, amount_func_minor) VALUES (?1, ?2, ?3, ?4, 'CZK', ?4)",
            params![rid, n as i64 + 1, account(code), amount],
        )
        .unwrap();
    }
    let attempt = conn.execute(
        "UPDATE journal_entry SET status = 'posted', period_id = ?2, posted_seq = 2, posted_at = 'x', chain_hash = ?3 WHERE id = ?1",
        params![rid, q3, "0".repeat(64)],
    );
    assert!(rule(LedgerError::from(attempt.unwrap_err())).contains("mirrors every line"));
    assert!(
        raw_rule(&conn, &format!("UPDATE journal_entry SET source_kind = 'manual', reverses_id = NULL WHERE id = {rid}"))
            .contains("origin can't change")
    );
}

// ---------------------------------------------------------- periods + close

#[test]
fn a_period_moves_open_closing_closed_and_then_stays_closed() {
    let Ledger { conn, q3, q4 } = ledger();
    assert!(raw_rule(&conn, &format!("UPDATE period SET state = 'closed', closed_by = 'x', closed_at = 'x' WHERE id = {q3}"))
        .contains("open → closing → closed"));
    assert!(matches!(
        close_period(&conn, q3, STANDARD_CHECKS, "user"),
        Err(LedgerError::WrongPeriodState {
            is: PeriodState::Open,
            needs: PeriodState::Closing,
            ..
        })
    ));

    assert_eq!(begin_close(&conn, q3).unwrap().state, PeriodState::Closing);
    // A closing period still takes adjustments.
    post(&conn, "2026-09-30", "accrual", pair("518", "321", 120_000));
    assert_eq!(reopen_period(&conn, q3).unwrap().state, PeriodState::Open);
    begin_close(&conn, q3).unwrap();

    // Q4 can't close before Q3.
    begin_close(&conn, q4).unwrap();
    let blocked = close_period(&conn, q4, STANDARD_CHECKS, "user").unwrap_err();
    let LedgerError::CloseBlocked(report) = blocked else {
        panic!("expected a blocked close")
    };
    let failed: Vec<&str> = report.failures().map(|f| f.key).collect();
    assert_eq!(failed, ["earlier_periods_closed"]);
    assert_eq!(
        report.results[0].problems,
        ["2026-07-01 – 2026-09-30 is still closing"]
    );

    let report = close_period(&conn, q3, STANDARD_CHECKS, "jan.novak").unwrap();
    assert!(report.passed());
    assert_eq!(report.results.len(), STANDARD_CHECKS.len());
    let closed: Period = get_period(&conn, q3).unwrap();
    assert_eq!(closed.state, PeriodState::Closed);
    assert_eq!(closed.closed_by.as_deref(), Some("jan.novak"));
    assert!(closed.closed_at.is_some());
    close_period(&conn, q4, STANDARD_CHECKS, "jan.novak").unwrap();

    // Closed is final, even for raw SQL, and nothing posts into it.
    assert!(matches!(
        begin_close(&conn, q3),
        Err(LedgerError::WrongPeriodState { .. })
    ));
    assert!(
        raw_rule(
            &conn,
            &format!("UPDATE period SET state = 'open' WHERE id = {q3}")
        )
        .contains("final")
    );
    assert!(
        raw_rule(&conn, &format!("DELETE FROM period WHERE id = {q3}")).contains("never deleted")
    );
    let id = create_draft(
        &conn,
        &NewEntry {
            date: "2026-08-08".into(),
            source_kind: SourceKind::Manual,
            source_ref: None,
            memo: String::new(),
            created_by: "user".into(),
            lines: pair("501", "221", 1),
        },
    )
    .unwrap();
    assert!(matches!(
        post_entry(&conn, id, None),
        Err(LedgerError::PeriodClosed(_))
    ));
    assert_eq!(list_periods(&conn).unwrap().len(), 2);
}

/// Stands in for a module's check, such as the bank tie-out.
struct BankTiedOut {
    difference_minor: i64,
}

impl CloseCheck for BankTiedOut {
    fn key(&self) -> &'static str {
        "bank_tied_out"
    }
    fn title(&self) -> &'static str {
        "Bank statement ties out"
    }
    fn run(&self, _conn: &Connection, period: &Period) -> Result<Vec<String>, LedgerError> {
        Ok(if self.difference_minor == 0 {
            Vec::new()
        } else {
            vec![format!(
                "221 differs from the statement at {} by {}",
                period.ends_on, self.difference_minor
            )]
        })
    }
}

#[test]
fn the_close_blocks_while_any_check_fails() {
    let Ledger { conn, q3, .. } = ledger();
    post(&conn, "2026-08-01", "rent", pair("518", "221", 1_500_000));
    let draft = create_draft(
        &conn,
        &NewEntry {
            date: "2026-09-10".into(),
            source_kind: SourceKind::Manual,
            source_ref: None,
            memo: "unfinished phone bill".into(),
            created_by: "user".into(),
            lines: pair("518", "221", 89_900),
        },
    )
    .unwrap();
    begin_close(&conn, q3).unwrap();

    let bank_off = BankTiedOut {
        difference_minor: 89_900,
    };
    let checks: Vec<&dyn CloseCheck> = STANDARD_CHECKS
        .iter()
        .copied()
        .chain([&bank_off as &dyn CloseCheck])
        .collect();
    let preview = run_close_checks(&conn, q3, &checks).unwrap();
    let failed: Vec<&str> = preview.failures().map(|f| f.key).collect();
    assert_eq!(failed, ["no_drafts", "bank_tied_out"]);
    assert_eq!(
        preview.results[1].problems,
        ["draft dated 2026-09-10: unfinished phone bill"]
    );

    let err = close_period(&conn, q3, &checks, "user").unwrap_err();
    assert!(
        err.to_string()
            .contains("No draft entries, Bank statement ties out"),
        "{err}"
    );
    assert_eq!(get_period(&conn, q3).unwrap().state, PeriodState::Closing);
    // The trigger refuses too, for a close that skips the checks.
    assert!(raw_rule(&conn, &format!("UPDATE period SET state = 'closed', closed_by = 'x', closed_at = 'x', chain_seq_at_close = 1, chain_head_at_close = (SELECT chain_hash FROM journal_entry WHERE posted_seq = 1) WHERE id = {q3}"))
        .contains("draft entries"));

    post_entry(&conn, draft, None).unwrap();
    let still = close_period(&conn, q3, &checks, "user").unwrap_err();
    assert!(matches!(still, LedgerError::CloseBlocked(r) if r.failures().count() == 1));

    let bank_ok = BankTiedOut {
        difference_minor: 0,
    };
    let checks: Vec<&dyn CloseCheck> = STANDARD_CHECKS
        .iter()
        .copied()
        .chain([&bank_ok as &dyn CloseCheck])
        .collect();
    assert!(close_period(&conn, q3, &checks, "user").unwrap().passed());
    assert!(matches!(
        close_period(&conn, q3, &checks, " "),
        Err(LedgerError::InvalidEntry(_))
    ));
}

// --------------------------------------------------------------- hash chain

fn post_many(conn: &Connection, n: usize) -> Vec<(i64, String)> {
    (0..n)
        .map(|i| {
            let id = post(
                conn,
                "2026-08-15",
                &format!("entry {i}"),
                pair("501", "221", 1_000 + i as i64),
            );
            (id, get_entry(conn, id).expect("entry").uid)
        })
        .collect()
}

/// Drops the triggers that freeze posted rows, as someone editing the file would.
fn unguard(conn: &Connection) {
    conn.execute_batch(
        "DROP TRIGGER journal_entry_posted_is_frozen;
         DROP TRIGGER posting_update_only_on_draft;
         DROP TRIGGER posting_delete_only_on_draft;
         DROP TRIGGER journal_entry_posted_never_deleted;",
    )
    .expect("drop guards");
}

#[test]
fn every_posted_entry_carries_a_verifiable_link() {
    let Ledger { conn, .. } = ledger();
    let empty = verify_chain(&conn).unwrap();
    assert!(empty.is_intact());
    assert_eq!((empty.entries_checked, empty.head), (0, None));

    let entries = post_many(&conn, 20);
    let report = verify_chain(&conn).unwrap();
    assert!(report.is_intact());
    assert_eq!(report.entries_checked, 20);
    let last = get_entry(&conn, entries[19].0).unwrap();
    assert_eq!(report.head, last.chain_hash);
    let hashes: std::collections::HashSet<_> = entries
        .iter()
        .map(|(id, _)| get_entry(&conn, *id).unwrap().chain_hash.unwrap())
        .collect();
    assert_eq!(hashes.len(), 20);
}

#[test]
fn the_verifier_names_the_first_entry_with_a_single_tampered_byte() {
    // One byte in a memo.
    let Ledger { conn, .. } = ledger();
    let entries = post_many(&conn, 12);
    unguard(&conn);
    conn.execute(
        "UPDATE journal_entry SET memo = 'entry 8' WHERE id = ?1",
        [entries[7].0],
    )
    .unwrap();
    // "entry 7" → "entry 8": a single byte.
    let report = verify_chain(&conn).unwrap();
    let b = report.first_break.unwrap();
    assert_eq!((b.posted_seq, b.kind), (8, ChainBreakKind::Altered));
    assert_eq!(b.entry_uid.as_deref(), Some(entries[7].1.as_str()));
    assert_eq!(report.entries_checked, 7);
    assert!(b.to_string().starts_with("posted entry 8 ("), "{b}");

    // One unit of one amount, on both lines so the entry still balances.
    let Ledger { conn, .. } = ledger();
    let entries = post_many(&conn, 12);
    unguard(&conn);
    conn.execute(
        "UPDATE posting SET amount_minor = amount_minor + 1, amount_func_minor = amount_func_minor + 1 WHERE entry_id = ?1 AND line_no = 1",
        [entries[2].0],
    )
    .unwrap();
    conn.execute(
        "UPDATE posting SET amount_minor = amount_minor - 1, amount_func_minor = amount_func_minor - 1 WHERE entry_id = ?1 AND line_no = 2",
        [entries[2].0],
    )
    .unwrap();
    assert_eq!(
        verify_chain(&conn).unwrap().first_break.unwrap().posted_seq,
        3
    );

    // One hex digit of a stored hash.
    let Ledger { conn, .. } = ledger();
    let entries = post_many(&conn, 5);
    unguard(&conn);
    let hash = get_entry(&conn, entries[4].0).unwrap().chain_hash.unwrap();
    let flipped = format!(
        "{}{}",
        if hash.starts_with('0') { '1' } else { '0' },
        &hash[1..]
    );
    conn.execute(
        "UPDATE journal_entry SET chain_hash = ?2 WHERE id = ?1",
        params![entries[4].0, flipped],
    )
    .unwrap();
    assert_eq!(
        verify_chain(&conn).unwrap().first_break.unwrap().posted_seq,
        5
    );

    // A deleted entry.
    let Ledger { conn, .. } = ledger();
    let entries = post_many(&conn, 6);
    unguard(&conn);
    conn.execute("DELETE FROM journal_entry WHERE id = ?1", [entries[3].0])
        .unwrap();
    let b = verify_chain(&conn).unwrap().first_break.unwrap();
    assert_eq!(
        (b.posted_seq, b.kind, b.entry_uid),
        (4, ChainBreakKind::Missing, None)
    );
}

#[test]
fn random_single_byte_tampering_is_always_caught_at_the_right_entry() {
    let Ledger { conn, .. } = ledger();
    let entries = post_many(&conn, 60);
    unguard(&conn);
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..40 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let pick = (state % 60) as usize;
        let (id, _) = entries[pick];
        conn.execute_batch("SAVEPOINT t").unwrap();
        match state % 3 {
            0 => conn.execute(
                "UPDATE journal_entry SET memo = memo || 'x' WHERE id = ?1",
                [id],
            ),
            1 => conn.execute(
                "UPDATE journal_entry SET entry_date = '2026-08-16' WHERE id = ?1",
                [id],
            ),
            _ => conn.execute(
                "UPDATE posting SET memo = 'x' WHERE entry_id = ?1 AND line_no = 2",
                [id],
            ),
        }
        .unwrap();
        let b = verify_chain(&conn)
            .unwrap()
            .first_break
            .expect("tampering detected");
        assert_eq!(b.posted_seq, pick as i64 + 1);
        conn.execute_batch("ROLLBACK TO t; RELEASE t").unwrap();
    }
    assert!(verify_chain(&conn).unwrap().is_intact());
}

#[test]
fn closing_seals_the_chain_head_so_cutting_the_tail_is_caught() {
    let Ledger { conn, q3, .. } = ledger();
    let entries = post_many(&conn, 5);
    begin_close(&conn, q3).unwrap();
    close_period(&conn, q3, STANDARD_CHECKS, "user").unwrap();
    let sealed: (i64, String) = conn
        .query_row(
            "SELECT chain_seq_at_close, chain_head_at_close FROM period WHERE id = ?1",
            [q3],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(sealed.0, 5);
    assert_eq!(
        Some(sealed.1),
        get_entry(&conn, entries[4].0).unwrap().chain_hash
    );
    post(&conn, "2026-10-05", "q4", pair("501", "221", 7));
    assert!(verify_chain(&conn).unwrap().is_intact());

    // Removing the last two Q3 entries and the Q4 one leaves a chain that
    // verifies on its own, but not against the seal.
    unguard(&conn);
    conn.execute_batch("DELETE FROM journal_entry WHERE posted_seq >= 4")
        .unwrap();
    let b = verify_chain(&conn).unwrap().first_break.unwrap();
    assert_eq!((b.posted_seq, b.kind), (5, ChainBreakKind::SealMismatch));
}

#[test]
fn a_broken_chain_blocks_the_close() {
    let Ledger { conn, q3, .. } = ledger();
    let entries = post_many(&conn, 3);
    conn.execute_batch("DROP TRIGGER journal_entry_posted_is_frozen")
        .unwrap();
    conn.execute(
        "UPDATE journal_entry SET memo = 'edited' WHERE id = ?1",
        [entries[1].0],
    )
    .unwrap();
    begin_close(&conn, q3).unwrap();
    let LedgerError::CloseBlocked(report) =
        close_period(&conn, q3, STANDARD_CHECKS, "user").unwrap_err()
    else {
        panic!("expected a blocked close");
    };
    let chain = report
        .failures()
        .find(|f| f.key == "chain_intact")
        .expect("chain check failed");
    assert!(
        chain.problems[0].starts_with("posted entry 2 ("),
        "{:?}",
        chain.problems
    );
    assert!(chain.problems[0].ends_with("changed after it was posted"));
}

#[test]
fn a_replayed_identity_makes_the_chain_reproducible() {
    let uid = |n: u32| format!("00000000-0000-7000-8000-{n:012}");
    let build = || {
        let Ledger { conn, .. } = ledger();
        for (n, date) in [(1, "2026-08-01"), (2, "2026-08-02")] {
            let id = skyla_ledger::create_draft_as(
                &conn,
                &NewEntry {
                    date: date.into(),
                    source_kind: SourceKind::Manual,
                    source_ref: None,
                    memo: format!("entry {n}"),
                    created_by: "user".into(),
                    lines: pair("501", "221", 100),
                },
                &uid(n),
            )
            .unwrap();
            skyla_ledger::post_entry_at(&conn, id, None, Some(&format!("{date}T18:00:00.000Z")))
                .unwrap();
        }
        let replay = skyla_ledger::Replay {
            uid: &uid(3),
            posted_at: "2026-08-03T18:00:00.000Z",
        };
        skyla_ledger::reverse_entry_at(&conn, 1, "2026-08-03", "user", None, Some(replay)).unwrap();
        assert_eq!(get_entry(&conn, 3).unwrap().uid, uid(3));
        verify_chain(&conn).unwrap()
    };
    let (a, b) = (build(), build());
    assert!(a.is_intact());
    assert_eq!(a.head, b.head);
    assert_eq!(a.entries_checked, 3);

    let Ledger { conn, .. } = ledger();
    let draft = NewEntry {
        date: "2026-08-01".into(),
        source_kind: SourceKind::Manual,
        source_ref: None,
        memo: String::new(),
        created_by: "user".into(),
        lines: pair("501", "221", 100),
    };
    for bad in [
        "not-a-uuid",
        "00000000-0000-7000-8000-00000000000A",
        "{00000000-0000-7000-8000-000000000001}",
    ] {
        assert!(
            matches!(
                skyla_ledger::create_draft_as(&conn, &draft, bad),
                Err(LedgerError::InvalidEntry(_))
            ),
            "{bad}"
        );
    }
    let id = skyla_ledger::create_draft_as(&conn, &draft, &uid(9)).unwrap();
    assert!(matches!(
        skyla_ledger::create_draft_as(&conn, &draft, &uid(9)),
        Err(LedgerError::Sql(_))
    ));
    for bad in [
        "2026-08-01",
        "2026-08-01T25:00:00.000Z",
        "2026-02-30T10:00:00.000Z",
        "2026-08-01 10:00:00.000Z",
    ] {
        assert!(
            matches!(
                skyla_ledger::post_entry_at(&conn, id, None, Some(bad)),
                Err(LedgerError::InvalidEntry(_))
            ),
            "{bad}"
        );
    }
    skyla_ledger::post_entry_at(&conn, id, None, Some("2026-08-01T09:30:00.250Z")).unwrap();
    assert_eq!(
        get_entry(&conn, id).unwrap().posted_at.as_deref(),
        Some("2026-08-01T09:30:00.250Z")
    );
}
