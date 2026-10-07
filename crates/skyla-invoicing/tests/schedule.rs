//! WP-15 acceptance: a simulated year of recurring invoices and reminders.
//!
//! Every day of 2026 the app runs its recurring templates and its reminder
//! queue, except June to August, when it stays closed. Payments arrive on
//! time, late, in part or never; one invoice is put on hold. The expected
//! dates were worked out by hand against the 2026 Czech calendar (weekends,
//! 3 and 6 April, 28 September, 17 November).

mod common;

use std::collections::BTreeMap;

use common::{books, draft, line};
use skyla_invoicing::dunning::{dunning_queue, hold_reminders, record_reminder, release_reminders};
use skyla_invoicing::recurring::{
    Frequency, Schedule, TemplateInput, create_template, occurrences, run_recurring,
    set_template_active, templates,
};
use skyla_invoicing::{DocKind, RepoRate, get};
use skyla_rules::date;

fn template(
    name: &str,
    frequency: Frequency,
    interval: u32,
    start: &str,
    end: Option<&str>,
    auto_issue: bool,
) -> TemplateInput {
    TemplateInput {
        name: name.into(),
        draft: draft(
            DocKind::Invoice,
            "FV",
            vec![line(
                &format!("{name} · {{month}}"),
                "1",
                "ks",
                250_000,
                "OUT21",
            )],
        ),
        schedule: Schedule {
            frequency,
            interval,
            start: start.into(),
            end: end.map(Into::into),
        },
        due_days: 14,
        auto_issue,
    }
}

#[test]
fn schedules_clamp_to_month_ends_and_stop_at_the_end_date() {
    let monthly = Schedule {
        frequency: Frequency::Monthly,
        interval: 1,
        start: "2026-01-31".into(),
        end: None,
    };
    assert_eq!(
        occurrences(&monthly, "2026-06-30").expect("ok"),
        [
            "2026-01-31",
            "2026-02-28",
            "2026-03-31",
            "2026-04-30",
            "2026-05-31",
            "2026-06-30"
        ]
    );
    let leap = Schedule {
        frequency: Frequency::Yearly,
        interval: 1,
        start: "2028-02-29".into(),
        end: None,
    };
    assert_eq!(
        occurrences(&leap, "2032-12-31").expect("ok"),
        [
            "2028-02-29",
            "2029-02-28",
            "2030-02-28",
            "2031-02-28",
            "2032-02-29"
        ]
    );
    let fortnightly = Schedule {
        frequency: Frequency::Weekly,
        interval: 2,
        start: "2026-01-05".into(),
        end: Some("2026-03-31".into()),
    };
    assert_eq!(
        occurrences(&fortnightly, "2026-12-31").expect("ok"),
        [
            "2026-01-05",
            "2026-01-19",
            "2026-02-02",
            "2026-02-16",
            "2026-03-02",
            "2026-03-16",
            "2026-03-30"
        ]
    );
}

#[test]
fn a_simulated_year() {
    let b = books(true);
    let hosting = create_template(
        &b.conn,
        &template("Hosting", Frequency::Monthly, 1, "2026-01-31", None, true),
    )
    .expect("template");
    let retainer = create_template(
        &b.conn,
        &template(
            "Retainer",
            Frequency::Quarterly,
            1,
            "2026-02-15",
            None,
            false,
        ),
    )
    .expect("template");
    let coaching = create_template(
        &b.conn,
        &template(
            "Coaching",
            Frequency::Weekly,
            2,
            "2026-01-05",
            Some("2026-03-31"),
            false,
        ),
    )
    .expect("template");
    let repo = [RepoRate {
        effective_from: "2025-05-02".into(),
        rate: "3.50".parse().expect("rate"),
    }];

    // number → (issue date, tax point, due date)
    let mut issued: BTreeMap<String, (String, String, String)> = BTreeMap::new();
    let mut entry_of: BTreeMap<String, i64> = BTreeMap::new();
    let mut drafts: Vec<(i64, String)> = Vec::new();
    let mut reminders: Vec<(String, u8, String)> = Vec::new();
    let mut final_bodies: BTreeMap<String, String> = BTreeMap::new();

    let payments: BTreeMap<&str, Vec<(&str, i64)>> = BTreeMap::from([
        ("2026-02-10", vec![("2026-001", 302_500)]),
        ("2026-04-03", vec![("2026-002", 302_500)]),
        ("2026-06-20", vec![("2026-005", 100_000)]),
        ("2026-09-14", vec![("2026-006", 302_500)]),
        ("2026-09-25", vec![("2026-007", 302_500)]),
        ("2026-10-14", vec![("2026-009", 302_500)]),
        ("2026-11-20", vec![("2026-010", 302_500)]),
    ]);

    let start = date::parse("2026-01-01").expect("date");
    let end = date::parse("2026-12-31").expect("date");
    for day in start..=end {
        let today = date::format(day);
        // The bank statement carries every payment on its own date, whether
        // or not the app was open that day.
        for (number, minor) in payments.get(today.as_str()).into_iter().flatten() {
            b.pay(entry_of[*number], &today, *minor);
        }
        if ("2026-06-01".."2026-09-01").contains(&today.as_str()) {
            continue; // closed for the summer
        }
        if today == "2026-05-15" {
            let id = get_by_number(&b, "2026-004");
            hold_reminders(&b.conn, id, "Disputed hours", &today).expect("hold");
        }
        if today == "2026-10-01" {
            release_reminders(&b.conn, get_by_number(&b, "2026-004")).expect("release");
        }
        for run in run_recurring(&b.conn, &b.pack, &b.accounts, &today).expect("run") {
            assert_eq!(run.problem, None);
            let doc = get(&b.conn, &b.pack, run.document_id).expect("doc");
            match run.number {
                Some(number) => {
                    entry_of.insert(number.clone(), doc.entry_id.expect("posted"));
                    issued.insert(
                        number,
                        (
                            doc.issue_date.clone().expect("issued"),
                            doc.tax_point_date.clone().expect("tax point"),
                            doc.due_date.clone().expect("due"),
                        ),
                    );
                }
                None => drafts.push((run.template_id, run.occurrence)),
            }
        }
        assert!(
            run_recurring(&b.conn, &b.pack, &b.accounts, &today)
                .expect("rerun")
                .is_empty(),
            "a second run on {today} makes nothing"
        );
        for notice in dunning_queue(&b.conn, &b.pack, &repo, &today).expect("queue") {
            record_reminder(&b.conn, notice.document_id, notice.step, &today).expect("record");
            if notice.step == 3 {
                final_bodies.insert(notice.number.clone(), notice.body_cs.clone());
            }
            reminders.push((notice.number, notice.step, today.clone()));
        }
        assert!(
            dunning_queue(&b.conn, &b.pack, &repo, &today)
                .expect("queue")
                .is_empty(),
            "nothing goes out twice on {today}"
        );
    }

    // Twelve hosting invoices, numbered in calendar order. June to August
    // were caught up on 1 September: issued that day, taxed in their month.
    let expected = [
        ("2026-001", "2026-01-31", "2026-01-31", "2026-02-14"),
        ("2026-002", "2026-02-28", "2026-02-28", "2026-03-14"),
        ("2026-003", "2026-03-31", "2026-03-31", "2026-04-14"),
        ("2026-004", "2026-04-30", "2026-04-30", "2026-05-14"),
        ("2026-005", "2026-05-31", "2026-05-31", "2026-06-14"),
        ("2026-006", "2026-09-01", "2026-06-30", "2026-09-15"),
        ("2026-007", "2026-09-01", "2026-07-31", "2026-09-15"),
        ("2026-008", "2026-09-01", "2026-08-31", "2026-09-15"),
        ("2026-009", "2026-09-30", "2026-09-30", "2026-10-14"),
        ("2026-010", "2026-10-31", "2026-10-31", "2026-11-14"),
        ("2026-011", "2026-11-30", "2026-11-30", "2026-12-14"),
        ("2026-012", "2026-12-31", "2026-12-31", "2027-01-14"),
    ];
    let got: Vec<(&str, &str, &str, &str)> = issued
        .iter()
        .map(|(n, (i, t, d))| (n.as_str(), i.as_str(), t.as_str(), d.as_str()))
        .collect();
    assert_eq!(got, expected);
    let june = get(&b.conn, &b.pack, get_by_number(&b, "2026-006")).expect("doc");
    assert_eq!(june.lines[0].input.description, "Hosting · červen 2026");

    // Drafts only for the other two; August's retainer caught up in September.
    let of = |t: i64| -> Vec<&str> {
        drafts
            .iter()
            .filter(|(id, _)| *id == t)
            .map(|(_, d)| d.as_str())
            .collect()
    };
    assert_eq!(
        of(retainer),
        ["2026-02-15", "2026-05-15", "2026-08-15", "2026-11-15"]
    );
    assert_eq!(
        of(coaching),
        [
            "2026-01-05",
            "2026-01-19",
            "2026-02-02",
            "2026-02-16",
            "2026-03-02",
            "2026-03-16",
            "2026-03-30"
        ]
    );
    assert_eq!(of(hosting), Vec::<&str>::new());

    // Reminders: 3, 14 and 30 days after the due date, on working days.
    let r = |n: &str, s: u8, d: &str| (n.to_owned(), s, d.to_owned());
    let mut want = vec![
        // Due Saturday 14 March; paid 3 April, before the final notice.
        r("2026-002", 1, "2026-03-17"),
        r("2026-002", 2, "2026-03-30"),
        // Never paid.
        r("2026-003", 1, "2026-04-17"),
        r("2026-003", 2, "2026-04-28"),
        r("2026-003", 3, "2026-05-14"),
        // Closed all summer: only the final notice goes out on reopening.
        r("2026-005", 3, "2026-09-01"),
        r("2026-007", 1, "2026-09-18"),
        r("2026-008", 1, "2026-09-18"),
        r("2026-008", 2, "2026-09-29"),
        // On hold from 15 May; released 1 October, straight to the final notice.
        r("2026-004", 3, "2026-10-01"),
        r("2026-008", 3, "2026-10-15"),
        // 17 November is a holiday.
        r("2026-010", 1, "2026-11-18"),
        r("2026-011", 1, "2026-12-17"),
        r("2026-011", 2, "2026-12-28"),
    ];
    want.sort_by(|a, b| a.2.cmp(&b.2).then(a.0.cmp(&b.0)));
    reminders.sort_by(|a, b| a.2.cmp(&b.2).then(a.0.cmp(&b.0)));
    assert_eq!(reminders, want);

    // The final notices state the statutory interest to their day.
    // 2026-003: 3 025 × 11.5 % × 30 / 365 = 28.5924… → 28,59.
    assert!(
        final_bodies["2026-003"].contains("úrok z prodlení 28,59\u{a0}Kč (sazba 11,5 % ročně"),
        "{}",
        final_bodies["2026-003"]
    );
    // 2026-005: 3 025 × 11.5 % × 6 / 365 = 5.7184… → 5,72, then
    // 2 025 × 11.5 % × 73 / 365 = 46.575 → 46,58 (half up); 52,30 in all.
    assert!(
        final_bodies["2026-005"].contains("úrok z prodlení 52,30\u{a0}Kč"),
        "{}",
        final_bodies["2026-005"]
    );
    assert!(final_bodies["2026-005"].contains("Zbývá uhradit 2\u{a0}025,00\u{a0}Kč"));
    assert!(
        final_bodies["2026-005"].contains("1\u{a0}200,00\u{a0}Kč"),
        "the recovery cost"
    );

    // Pausing keeps the schedule; the next occurrence waits for a resume.
    set_template_active(&b.conn, hosting, false).expect("pause");
    let t = templates(&b.conn).expect("templates");
    assert!(!t[0].active);
    assert_eq!(t[0].next.as_deref(), Some("2027-01-31"));
    assert_eq!(t[2].next, None, "coaching ended in March");
}

fn get_by_number(b: &common::Books, number: &str) -> i64 {
    b.conn
        .query_row("SELECT id FROM document WHERE number = ?1", [number], |r| {
            r.get(0)
        })
        .expect("document")
}
