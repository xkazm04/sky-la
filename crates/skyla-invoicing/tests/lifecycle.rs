//! WP-11 acceptance: the document lifecycle, gapless numbering, immutability
//! of issued documents, credit notes restoring the balance, and advances.

use proptest::prelude::*;
use rusqlite::Connection;
use skyla_invoicing::{
    Accounts, Customer, DocKind, DraftInput, InvoicingError, LineInput, Settlement, apply_schema,
    create_draft, define_series, delete_draft, draft_credit_note, get, import_issued, issue,
    series_gaps, state, update_draft,
};
use skyla_ledger::{
    ChartSpec, NewEntry, NewLine, SourceKind, link_settlement, open_period, post_entry,
};
use skyla_money::{Currency, Money};
use skyla_rules::Pack;

const CZ_CHART: &str = include_str!("../../../rules/cz/chart.toml");

struct Books {
    conn: Connection,
    pack: Pack,
    accounts: Accounts,
}

fn books() -> Books {
    let conn = Connection::open_in_memory().expect("db");
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .expect("pragma");
    skyla_ledger::apply_schema(&conn).expect("ledger schema");
    apply_schema(&conn).expect("invoicing schema");
    skyla_ledger::seed_chart(&conn, &ChartSpec::from_toml(CZ_CHART).expect("chart")).expect("seed");
    skyla_ledger::set_functional_currency(&conn, Currency::CZK).expect("ccy");
    open_period(&conn, "2026-01-01", "2026-12-31").expect("2026");
    open_period(&conn, "2027-01-01", "2027-12-31").expect("2027");
    define_series(&conn, "FV", DocKind::Invoice, "{YYYY}-{NNN}", "Invoices").expect("series");
    define_series(
        &conn,
        "OD",
        DocKind::CreditNote,
        "OD{YY}{NNNN}",
        "Credit notes",
    )
    .expect("series");
    define_series(
        &conn,
        "ZF",
        DocKind::Advance,
        "ZF{YYYY}-{NN}",
        "Advance invoices",
    )
    .expect("series");
    define_series(
        &conn,
        "DZ",
        DocKind::AdvanceTax,
        "DZ{YYYY}-{NN}",
        "Advance tax documents",
    )
    .expect("series");
    Books {
        conn,
        pack: Pack::cz_2026().expect("pack"),
        accounts: Accounts::cz(),
    }
}

fn czk(minor: i64) -> Money {
    Money::new(minor, Currency::CZK)
}

fn northwind() -> Customer {
    Customer {
        name: "Northwind Traders s.r.o.".into(),
        ico: Some("12345678".into()),
        dic: Some("CZ12345678".into()),
        address: Some("Vinohradská 1, 120 00 Praha 2".into()),
    }
}

fn line(description: &str, quantity: &str, unit_price_minor: i64) -> LineInput {
    LineInput {
        description: description.into(),
        quantity: quantity.into(),
        unit: "h".into(),
        unit_price_minor,
        vat_code: "OUT21".into(),
        account: None,
    }
}

fn invoice(lines: Vec<LineInput>) -> DraftInput {
    DraftInput {
        kind: DocKind::Invoice,
        series: "FV".into(),
        customer: northwind(),
        due_date: Some("2026-09-29".into()),
        tax_point_date: None,
        note: String::new(),
        lines,
        related_id: None,
        advances: Vec::new(),
    }
}

fn balance(conn: &Connection, code: &str) -> i64 {
    conn.query_row(
        "SELECT coalesce(sum(p.amount_func_minor), 0) FROM posting p JOIN account a ON a.id = p.account_id
         JOIN journal_entry e ON e.id = p.entry_id WHERE a.code = ?1 AND e.status = 'posted'",
        [code],
        |r| r.get(0),
    )
    .expect("balance")
}

fn pay(b: &Books, invoice_entry: i64, date: &str, minor: i64) {
    let id = skyla_ledger::create_draft(
        &b.conn,
        &NewEntry {
            date: date.into(),
            source_kind: SourceKind::Bank,
            source_ref: None,
            memo: "Platba".into(),
            created_by: "user".into(),
            lines: vec![
                NewLine::debit("221", czk(minor)),
                NewLine::credit("311", czk(minor)).expect("credit"),
            ],
        },
    )
    .expect("payment draft");
    link_settlement(&b.conn, id, invoice_entry, czk(minor)).expect("settle");
    post_entry(&b.conn, id, None).expect("post payment");
}

#[test]
fn issuing_numbers_freezes_and_posts_the_invoice() {
    let b = books();
    let id = create_draft(
        &b.conn,
        &invoice(vec![line("Product design · September", "56", 125_000)]),
    )
    .expect("ok");
    let draft = get(&b.conn, &b.pack, id).expect("ok");
    assert!(!draft.issued);
    assert_eq!(
        (draft.totals.base, draft.totals.vat, draft.totals.gross),
        (czk(7_000_000), czk(1_470_000), czk(8_470_000))
    );

    let issued = issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-15", None).expect("ok");
    assert_eq!(issued.number, "2026-001");
    let doc = get(&b.conn, &b.pack, id).expect("ok");
    assert!(doc.issued);
    assert_eq!(doc.pack.as_deref(), Some("cz-2026@2026.1"));
    assert_eq!(doc.tax_point_date.as_deref(), Some("2026-09-15"));

    let entry = skyla_ledger::get_entry(&b.conn, issued.entry_id.expect("ok")).expect("ok");
    let lines: Vec<(&str, i64, Option<&str>)> = entry
        .lines
        .iter()
        .map(|l| {
            (
                l.account.as_str(),
                l.functional.minor(),
                l.vat_code.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        lines,
        [
            ("311", 8_470_000, None),
            ("602", -7_000_000, Some("OUT21")),
            ("343", -1_470_000, Some("OUT21"))
        ]
    );
    assert_eq!(entry.source_ref.as_deref(), Some("2026-001"));
    assert_eq!(entry.memo, "Faktura 2026-001 Northwind Traders s.r.o.");
    assert_eq!(
        state(&b.conn, &b.pack, id).expect("ok").settlement,
        Settlement::Open
    );

    // The next number follows; a new year starts again at 001.
    let second =
        create_draft(&b.conn, &invoice(vec![line("Workshop", "1", 2_000_000)])).expect("ok");
    assert_eq!(
        issue(&b.conn, &b.pack, &b.accounts, second, "2026-09-20", None)
            .expect("ok")
            .number,
        "2026-002"
    );
    let mut next_year = invoice(vec![line("January", "10", 125_000)]);
    next_year.due_date = Some("2027-01-20".into());
    let third = create_draft(&b.conn, &next_year).expect("ok");
    assert_eq!(
        issue(&b.conn, &b.pack, &b.accounts, third, "2027-01-04", None)
            .expect("ok")
            .number,
        "2027-001"
    );
    assert!(series_gaps(&b.conn, "FV").expect("ok").is_empty());
}

#[test]
fn issued_documents_never_change_even_through_raw_sql() {
    let b = books();
    let id = create_draft(&b.conn, &invoice(vec![line("Design", "1", 100_000)])).expect("ok");
    issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-15", None).expect("ok");
    assert!(matches!(
        update_draft(&b.conn, id, &invoice(vec![line("Edited", "1", 1)])),
        Err(InvoicingError::Issued(_))
    ));
    assert!(matches!(
        delete_draft(&b.conn, id),
        Err(InvoicingError::Issued(_))
    ));
    assert!(matches!(
        issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-16", None),
        Err(InvoicingError::Issued(_))
    ));
    for sql in [
        "UPDATE document SET customer_name = 'Someone else'",
        "DELETE FROM document",
        "UPDATE document_line SET unit_price_minor = 1",
        "DELETE FROM document_line",
        "UPDATE document_total SET vat_minor = 0",
        "DELETE FROM document_total",
    ] {
        let err = b.conn.execute_batch(sql).expect_err(sql);
        assert!(
            matches!(InvoicingError::from(err), InvoicingError::Rule(_)),
            "{sql}"
        );
    }
    let doc = get(&b.conn, &b.pack, id).expect("ok");
    assert_eq!(doc.customer.name, "Northwind Traders s.r.o.");
    assert_eq!(doc.totals.vat, czk(21_000));
}

#[test]
fn a_full_credit_note_restores_the_balance() {
    let b = books();
    let id = create_draft(
        &b.conn,
        &invoice(vec![
            line("Design", "56", 125_000),
            line("Illustration", "3", 99_999),
        ]),
    )
    .expect("ok");
    let issued = issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-15", None).expect("ok");
    let gross = get(&b.conn, &b.pack, id).expect("ok").totals.gross.minor();
    assert_eq!(balance(&b.conn, "311"), gross);

    let credit =
        draft_credit_note(&b.conn, &b.pack, id, "OD", None, "Cancelled order").expect("ok");
    let note = issue(&b.conn, &b.pack, &b.accounts, credit, "2026-09-20", None).expect("ok");
    assert_eq!(note.number, "OD260001");
    for code in ["311", "602", "343"] {
        assert_eq!(balance(&b.conn, code), 0, "{code}");
    }
    let s = state(&b.conn, &b.pack, id).expect("ok");
    assert_eq!(s.settlement, Settlement::Credited);
    assert_eq!((s.credited.minor(), s.open.minor()), (gross, 0));
    let entry = skyla_ledger::get_entry(&b.conn, note.entry_id.expect("ok")).expect("ok");
    assert!(entry.memo.starts_with(&format!(
        "Opravný daňový doklad OD260001 k faktuře {}",
        issued.number
    )));

    // Nothing is left to credit.
    let again = draft_credit_note(&b.conn, &b.pack, id, "OD", None, "Twice").expect("ok");
    let err = issue(&b.conn, &b.pack, &b.accounts, again, "2026-09-21", None).expect_err("refused");
    assert!(
        matches!(err, InvoicingError::Ledger(skyla_ledger::LedgerError::Rule(ref m)) if m.contains("exceed")),
        "{err:?}"
    );
    // And the failed issue left no trace: the draft is still a draft, numbers unused.
    assert!(!get(&b.conn, &b.pack, again).expect("ok").issued);
    assert!(series_gaps(&b.conn, "OD").expect("ok").is_empty());
}

#[test]
fn payments_and_partial_credits_settle_the_invoice() {
    let b = books();
    let id = create_draft(&b.conn, &invoice(vec![line("Design", "100", 100_000)])).expect("ok");
    let entry = issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-15", None)
        .expect("ok")
        .entry_id
        .expect("ok");
    // 121 000,00 gross. Credit 10 hours, pay 50 000, then the rest.
    let credit = draft_credit_note(
        &b.conn,
        &b.pack,
        id,
        "OD",
        Some(vec![line("Design (10 h not delivered)", "-10", 100_000)]),
        "",
    )
    .expect("ok");
    issue(&b.conn, &b.pack, &b.accounts, credit, "2026-09-18", None).expect("ok");
    pay(&b, entry, "2026-09-25", 5_000_000);
    let s = state(&b.conn, &b.pack, id).expect("ok");
    assert_eq!(s.settlement, Settlement::PartlySettled);
    assert_eq!(
        (s.credited.minor(), s.paid.minor(), s.open.minor()),
        (1_210_000, 5_000_000, 5_890_000)
    );
    pay(&b, entry, "2026-09-30", 5_890_000);
    let s = state(&b.conn, &b.pack, id).expect("ok");
    assert_eq!(s.settlement, Settlement::Paid);
    assert_eq!(s.settled_on.as_deref(), Some("2026-09-30"));
    assert_eq!(balance(&b.conn, "311"), 0);
}

#[test]
fn an_advance_is_taxed_on_receipt_and_deducted_on_the_final_invoice() {
    let b = books();
    // Advance invoice: numbered, not posted.
    let mut adv = invoice(vec![line("Advance for the website", "1", 12_100_000)]);
    adv.kind = DocKind::Advance;
    adv.series = "ZF".into();
    let advance = create_draft(&b.conn, &adv).expect("ok");
    let a = issue(&b.conn, &b.pack, &b.accounts, advance, "2026-08-01", None).expect("ok");
    assert_eq!((a.number.as_str(), a.entry_id), ("ZF2026-01", None));

    // The bank receives 121 000,00 into advances received.
    let receipt = skyla_ledger::create_draft(
        &b.conn,
        &NewEntry {
            date: "2026-08-05".into(),
            source_kind: SourceKind::Bank,
            source_ref: None,
            memo: "Záloha".into(),
            created_by: "user".into(),
            lines: vec![
                NewLine::debit("221", czk(12_100_000)),
                NewLine::credit("324", czk(12_100_000)).expect("ok"),
            ],
        },
    )
    .expect("ok");
    post_entry(&b.conn, receipt, None).expect("ok");

    // Tax document on the received payment: VAT out of the gross.
    let mut tax = invoice(vec![LineInput {
        unit: String::new(),
        ..line("Received advance", "1", 12_100_000)
    }]);
    tax.kind = DocKind::AdvanceTax;
    tax.series = "DZ".into();
    tax.related_id = Some(advance);
    tax.due_date = None;
    tax.tax_point_date = Some("2026-08-05".into());
    let tax_doc = create_draft(&b.conn, &tax).expect("ok");
    issue(&b.conn, &b.pack, &b.accounts, tax_doc, "2026-08-06", None).expect("ok");
    let t = get(&b.conn, &b.pack, tax_doc).expect("ok");
    assert_eq!(
        (t.totals.base.minor(), t.totals.vat.minor()),
        (10_000_000, 2_100_000)
    );

    // Final invoice: 150 000 + VAT, less the advance.
    let mut fin = invoice(vec![line("Website", "1", 15_000_000)]);
    fin.advances = vec![tax_doc];
    fin.due_date = Some("2026-09-30".into());
    let final_id = create_draft(&b.conn, &fin).expect("ok");
    issue(&b.conn, &b.pack, &b.accounts, final_id, "2026-09-15", None).expect("ok");
    let s = state(&b.conn, &b.pack, final_id).expect("ok");
    assert_eq!(
        (s.gross.minor(), s.advances.minor(), s.open.minor()),
        (18_150_000, 12_100_000, 6_050_000)
    );

    assert_eq!(balance(&b.conn, "324"), 0);
    assert_eq!(balance(&b.conn, "311"), 6_050_000);
    assert_eq!(balance(&b.conn, "602"), -15_000_000);
    assert_eq!(balance(&b.conn, "343"), -3_150_000);

    // The VAT return: August taxes the advance, September the rest.
    let rules: Vec<skyla_ledger::VatRowRule> = b
        .pack
        .vat_code("OUT21")
        .expect("ok")
        .rows
        .iter()
        .map(|m| skyla_ledger::VatRowRule {
            vat_code: "OUT21".into(),
            row: m.row.clone(),
            part: if m.part == skyla_rules::RowPart::Base {
                skyla_ledger::VatPart::Base
            } else {
                skyla_ledger::VatPart::Tax
            },
            credit_positive: m.credit_positive,
        })
        .collect();
    let row1 = |from: &str, to: &str| {
        let l = skyla_ledger::vat_ledger(&b.conn, from, to, &["343"], &rules).expect("ok");
        (l.rows[0].base.minor(), l.rows[0].tax.minor())
    };
    assert_eq!(row1("2026-08-01", "2026-08-31"), (10_000_000, 2_100_000));
    assert_eq!(row1("2026-09-01", "2026-09-30"), (5_000_000, 1_050_000));
}

#[test]
fn bad_documents_are_refused_with_every_problem_listed() {
    let b = books();
    let mut bad = invoice(vec![line("", "0", -5), line("Credit", "-1", 100)]);
    bad.customer.name = " ".into();
    let Err(InvoicingError::Invalid(problems)) = create_draft(&b.conn, &bad) else {
        panic!("accepted")
    };
    let all = problems.join("\n");
    for expected in [
        "customer needs a name",
        "line 1: needs a description",
        "line 1: quantity is zero",
        "line 1: unit price can't be negative",
        "line 2: quantity must be positive",
    ] {
        assert!(all.contains(expected), "{expected}: {all}");
    }
    let mut wrong_series = invoice(vec![line("Design", "1", 1)]);
    wrong_series.series = "OD".into();
    assert!(matches!(
        create_draft(&b.conn, &wrong_series),
        Err(InvoicingError::Invalid(_))
    ));

    let mut late = invoice(vec![line("Design", "1", 100)]);
    late.due_date = Some("2026-09-01".into());
    let id = create_draft(&b.conn, &late).expect("ok");
    assert!(
        matches!(issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-15", None), Err(InvoicingError::Invalid(p)) if p[0].contains("due date"))
    );

    let mut unknown = invoice(vec![line("Design", "1", 100)]);
    unknown.lines[0].vat_code = "NOPE".into();
    let id = create_draft(&b.conn, &unknown).expect("ok");
    assert!(matches!(
        issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-15", None),
        Err(InvoicingError::Rules(_))
    ));
}

#[test]
fn imported_numbers_with_gaps_are_kept_and_reported() {
    let b = books();
    // Two invoices issued in another program as 2026-007 and 2026-009.
    for (number, base) in [("2026-007", 1_000_000), ("2026-009", 2_000_000)] {
        let gross = base + base * 21 / 100;
        let entry = skyla_ledger::create_draft(
            &b.conn,
            &NewEntry {
                date: "2026-03-01".into(),
                source_kind: SourceKind::Invoice,
                source_ref: Some(number.into()),
                memo: format!("Faktura {number}"),
                created_by: "import".into(),
                lines: vec![
                    NewLine::debit("311", czk(gross)),
                    NewLine::credit("602", czk(base)).expect("ok"),
                    NewLine::credit("343", czk(gross - base)).expect("ok"),
                ],
            },
        )
        .expect("ok");
        post_entry(&b.conn, entry, None).expect("ok");
        let mut input = invoice(vec![line("Imported", "1", base)]);
        input.due_date = Some("2026-03-15".into());
        import_issued(
            &b.conn,
            &b.pack,
            &b.accounts,
            &input,
            number,
            "2026-03-01",
            entry,
        )
        .expect("ok");
    }
    assert_eq!(
        series_gaps(&b.conn, "FV").expect("ok"),
        [
            "2026-001", "2026-002", "2026-003", "2026-004", "2026-005", "2026-006", "2026-008"
        ]
    );

    // An import whose entry doesn't match the document is refused.
    let mut input = invoice(vec![line("Imported", "1", 999)]);
    input.due_date = Some("2026-03-15".into());
    assert!(matches!(
        import_issued(
            &b.conn,
            &b.pack,
            &b.accounts,
            &input,
            "2026-010",
            "2026-03-01",
            1
        ),
        Err(InvoicingError::Invalid(_))
    ));
}

#[derive(Debug, Clone)]
enum Op {
    Draft(i64, i64),
    Issue,
    Edit,
    Credit,
    Pay,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (1_i64..200, 1_i64..2_000_000).prop_map(|(q, p)| Op::Draft(q, p)),
        Just(Op::Issue),
        Just(Op::Edit),
        Just(Op::Credit),
        Just(Op::Pay),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]
    #[test]
    fn any_sequence_keeps_numbers_gapless_issued_documents_fixed_and_receivables_equal_to_open(ops in prop::collection::vec(op(), 1..40)) {
        let b = books();
        let mut drafts: Vec<i64> = Vec::new();
        let mut issued: Vec<(i64, String)> = Vec::new(); // id, debug snapshot
        for (step, o) in ops.into_iter().enumerate() {
            match o {
                Op::Draft(q, p) => drafts.push(create_draft(&b.conn, &invoice(vec![line("Work", &q.to_string(), p)])).expect("ok")),
                Op::Edit => if let Some(&d) = drafts.last() {
                    update_draft(&b.conn, d, &invoice(vec![line("Edited work", "3", 33_333)])).expect("ok");
                },
                Op::Issue => if let Some(d) = drafts.pop() {
                    issue(&b.conn, &b.pack, &b.accounts, d, "2026-09-15", None).expect("ok");
                    let doc = get(&b.conn, &b.pack, d).expect("ok");
                    if doc.kind == DocKind::Invoice { issued.push((d, format!("{doc:?}"))); }
                },
                Op::Credit => if let Some((inv, _)) = issued.get(step % issued.len().max(1)) {
                    let open = state(&b.conn, &b.pack, *inv).expect("ok").open;
                    let full = get(&b.conn, &b.pack, *inv).expect("ok").totals.gross;
                    if open == full {
                        let c = draft_credit_note(&b.conn, &b.pack, *inv, "OD", None, "").expect("ok");
                        issue(&b.conn, &b.pack, &b.accounts, c, "2026-09-20", None).expect("ok");
                    }
                },
                Op::Pay => if let Some((inv, _)) = issued.get(step % issued.len().max(1)) {
                    let s = state(&b.conn, &b.pack, *inv).expect("ok");
                    let entry = get(&b.conn, &b.pack, *inv).expect("ok").entry_id.expect("ok");
                    if s.open.minor() > 0 { pay(&b, entry, "2026-09-25", s.open.minor()); }
                },
            }
        }
        prop_assert!(series_gaps(&b.conn, "FV").expect("ok").is_empty());
        prop_assert!(series_gaps(&b.conn, "OD").expect("ok").is_empty());
        let mut open_total = 0;
        for (id, snapshot) in &issued {
            prop_assert_eq!(&format!("{:?}", get(&b.conn, &b.pack, *id).expect("ok")), snapshot);
            open_total += state(&b.conn, &b.pack, *id).expect("ok").open.minor();
        }
        prop_assert_eq!(balance(&b.conn, "311"), open_total);
        prop_assert!(skyla_ledger::verify_chain(&b.conn).expect("ok").is_intact());
    }
}

#[test]
fn issuing_snapshots_the_supplier_and_asks_for_payment() {
    use skyla_invoicing::spayd::{document_payment, spayd};
    let b = books();
    let mut profile = skyla_invoicing::Supplier {
        name: "Jana Nováková".into(),
        ico: Some("25596641".into()),
        dic: Some("CZ25596641".into()),
        address: "Dlouhá 12\n110 00 Praha 1".into(),
        iban: Some("CZ5855000000001265098001".into()),
        bic: None,
        email: None,
        vat_payer: true,
        registration: "Fyzická osoba zapsaná v živnostenském rejstříku".into(),
    };
    skyla_invoicing::set_supplier(&b.conn, &profile).expect("profile");
    let id = create_draft(&b.conn, &invoice(vec![line("Design", "2", 125_000)])).expect("draft");
    let draft = get(&b.conn, &b.pack, id).expect("get");
    assert_eq!(
        draft.supplier.as_ref().map(|s| s.name.as_str()),
        Some("Jana Nováková")
    );
    assert_eq!(
        document_payment(&draft, draft.totals.gross),
        None,
        "drafts aren't payable"
    );

    issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-15", None).expect("issue");
    profile.address = "Nová 1\n602 00 Brno".into();
    skyla_invoicing::set_supplier(&b.conn, &profile).expect("move");

    let doc = get(&b.conn, &b.pack, id).expect("get");
    let snapshot = doc.supplier.as_ref().expect("snapshot");
    assert_eq!(
        snapshot.address, "Dlouhá 12\n110 00 Praha 1",
        "issued documents keep the old address"
    );
    let open = state(&b.conn, &b.pack, id).expect("state").open;
    let request = document_payment(&doc, open).expect("payable");
    assert_eq!(
        spayd(&request).expect("spayd"),
        "SPD*1.0*ACC:CZ5855000000001265098001*AM:3025.00*CC:CZK*DT:20260929*RN:Jana Nováková*MSG:Faktura 2026-001*X-VS:2026001"
    );
}

#[test]
fn a_supplier_outside_vat_charges_none() {
    let b = books();
    let profile = skyla_invoicing::Supplier {
        name: "Petr Malý".into(),
        ico: Some("25596641".into()),
        dic: None,
        address: "Krátká 3\n370 01 České Budějovice".into(),
        iban: None,
        bic: None,
        email: None,
        vat_payer: false,
        registration: String::new(),
    };
    skyla_invoicing::set_supplier(&b.conn, &profile).expect("profile");
    let charged = create_draft(&b.conn, &invoice(vec![line("Lekce", "2", 50_000)])).expect("draft");
    let err =
        issue(&b.conn, &b.pack, &b.accounts, charged, "2026-09-15", None).expect_err("refused");
    assert!(
        err.to_string().contains("isn't registered for VAT"),
        "{err}"
    );

    let plain = LineInput {
        vat_code: "NOVAT".into(),
        ..line("Lekce", "2", 50_000)
    };
    let id = create_draft(&b.conn, &invoice(vec![plain])).expect("draft");
    issue(&b.conn, &b.pack, &b.accounts, id, "2026-09-15", None).expect("issue");
    let doc = get(&b.conn, &b.pack, id).expect("get");
    assert_eq!(doc.totals.gross, czk(100_000));
    assert_eq!(doc.totals.vat, czk(0));
    assert_eq!(balance(&b.conn, "343"), 0, "no VAT posted");
    assert_eq!(balance(&b.conn, "602"), -100_000);
}
