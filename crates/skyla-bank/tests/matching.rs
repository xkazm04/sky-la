//! WP-18 acceptance: tie-out fails loudly on a missing line; every score is
//! exactly the sum of its contributions; and on a simulated year of a
//! freelancer's account the matcher accepts at least 85 % of all lines by
//! itself, with none accepted wrongly.

use std::collections::{BTreeMap, HashSet};

use proptest::prelude::*;
use skyla_bank::{
    Action, BankLine, Condition, Direction, OpenItem, Policy, Proposal, Rule, Statement,
    TieOutError, dedupe, normalise, parse, suggest, tie_out,
};
use skyla_money::{Currency, Money};

fn czk(minor: i64) -> Money {
    Money::new(minor, Currency::CZK)
}

fn sample(name: &str) -> Statement {
    let bytes = std::fs::read(format!(
        "{}/tests/samples/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("sample");
    parse(&bytes, None).expect("parse").remove(0)
}

#[test]
fn tie_out_fails_loudly_on_a_missing_or_doubled_line() {
    let s = sample("camt053-001.02.xml");
    let ok = tie_out(&s, Some(czk(83_051_000))).expect("ties out");
    assert_eq!(
        (ok.credits, ok.debits, ok.lines),
        (czk(25_762_000), czk(-12_000_000), 3)
    );

    let mut missing = s.clone();
    missing.lines.remove(1);
    let err = tie_out(&missing, None).expect_err("a line is missing");
    let TieOutError::Unbalanced { difference, .. } = &err else {
        panic!("{err}")
    };
    assert_eq!(*difference, czk(-12_000_000), "exactly the missing rent");
    assert!(err.to_string().contains("nothing was imported"), "{err}");

    let mut doubled = s.clone();
    doubled.lines.push(s.lines[0].clone());
    assert!(matches!(
        tie_out(&doubled, None),
        Err(TieOutError::Unbalanced { .. })
    ));

    let err =
        tie_out(&s, Some(czk(83_000_000))).expect_err("a statement is missing before this one");
    assert!(matches!(err, TieOutError::Gap { .. }), "{err}");
}

#[test]
fn an_overlapping_reimport_keeps_twin_lines_and_drops_known_ones() {
    let s = sample("camt053-001.02.xml");
    let mut lines = s.lines.clone();
    // Two identical card payments on one day are two lines.
    lines.push(lines[1].clone());
    let first = normalise("CZ2703000000000123454412", &lines);
    let known: HashSet<String> = first.iter().map(|l| l.key.clone()).collect();
    assert_eq!(known.len(), 4, "twins get their own keys");
    // The same file again plus one new line: only the new line is new.
    let mut again = lines.clone();
    again.push(BankLine {
        booking_date: "2026-09-30".into(),
        bank_ref: Some("NEW".into()),
        ..lines[0].clone()
    });
    let second = normalise("CZ27 0300 0000 0001 2345 4412", &again);
    let (new, old) = dedupe(&second, &known);
    assert_eq!((new.len(), old.len()), (1, 4));
}

/// A tiny deterministic generator, so the fixture year is the same on every run.
struct Rng(u64);
impl Rng {
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

struct Customer {
    name: &'static str,
    bank_name: &'static str,
    account: &'static str,
    other_account: &'static str,
    retainer: Option<i64>,
}

const CUSTOMERS: [Customer; 12] = [
    Customer {
        name: "Northwind Traders s.r.o.",
        bank_name: "NORTHWIND TRADERS SRO",
        account: "19-2000145399/0800",
        other_account: "2001234567/2010",
        retainer: Some(8_470_000),
    },
    Customer {
        name: "Acme Analytics a.s.",
        bank_name: "Acme Analytics a.s.",
        account: "987654321/0100",
        other_account: "1234567890/0300",
        retainer: None,
    },
    Customer {
        name: "Studio Brno s.r.o.",
        bank_name: "STUDIO BRNO",
        account: "2900112233/2010",
        other_account: "5566778899/0600",
        retainer: None,
    },
    Customer {
        name: "Kavárna U Mostu s.r.o.",
        bank_name: "Kavarna U Mostu",
        account: "1122334455/5500",
        other_account: "6677889900/0100",
        retainer: Some(605_000),
    },
    Customer {
        name: "Lumen Design a.s.",
        bank_name: "LUMEN DESIGN A.S.",
        account: "3344556677/0300",
        other_account: "7788990011/2700",
        retainer: None,
    },
    Customer {
        name: "Pivovar Hora s.r.o.",
        bank_name: "Pivovar Hora",
        account: "4455667788/0800",
        other_account: "8899001122/0710",
        retainer: None,
    },
    Customer {
        name: "Orbit Software s.r.o.",
        bank_name: "ORBIT SOFTWARE",
        account: "5566778800/2010",
        other_account: "9900112244/0100",
        retainer: Some(3_630_000),
    },
    Customer {
        name: "Zelená zahrada z.s.",
        bank_name: "Zelena zahrada",
        account: "6677880011/6100",
        other_account: "1011121314/0800",
        retainer: None,
    },
    Customer {
        name: "Tomáš Dvořák",
        bank_name: "DVORAK TOMAS",
        account: "7788991122/3030",
        other_account: "1516171819/0300",
        retainer: None,
    },
    Customer {
        name: "Vltava Media s.r.o.",
        bank_name: "Vltava Media",
        account: "8899002233/0100",
        other_account: "2021222324/5500",
        retainer: None,
    },
    Customer {
        name: "Brno Labs a.s.",
        bank_name: "BRNO LABS",
        account: "9900113344/0300",
        other_account: "2526272829/2010",
        retainer: None,
    },
    Customer {
        name: "Horák a syn s.r.o.",
        bank_name: "Horak a syn",
        account: "1029384756/0800",
        other_account: "3031323334/0600",
        retainer: None,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Behaviour {
    OnTime,
    NoSymbol,
    Typo,
    NewAccount,
    Instalments,
    Together,
    LessFee,
    Never,
}

/// What the line truly is.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Truth {
    Settles(Vec<(i64, i64)>),
    Rule(u32),
    Manual,
}

struct Day {
    line: BankLine,
    truth: Truth,
}

fn day_of(year_day: i64) -> String {
    skyla_rules::date::format(skyla_rules::date::parse("2026-01-01").expect("date") + year_day)
}

fn fixture_year() -> (Vec<OpenItem>, BTreeMap<String, Vec<Day>>, usize) {
    let mut rng = Rng(0x5eed_2026);
    let mut items = Vec::new();
    let mut days: BTreeMap<String, Vec<Day>> = BTreeMap::new();
    let mut push = |date: String, line: BankLine, truth: Truth| {
        days.entry(date.clone()).or_default().push(Day {
            line: BankLine {
                booking_date: date,
                ..line
            },
            truth,
        })
    };
    let blank = |amount: i64| BankLine {
        sequence: 0,
        booking_date: String::new(),
        value_date: None,
        amount: czk(amount),
        reversal: false,
        counterparty_name: None,
        counterparty_account: None,
        vs: None,
        ks: None,
        ss: None,
        message: None,
        bank_ref: None,
    };
    let mut seq = 0;
    let mut behaviours = Vec::new();
    // Invoices: most customers get one a month; retainers bill the same amount.
    for month in 0..12i64 {
        for (ci, c) in CUSTOMERS.iter().enumerate() {
            if rng.below(10) >= 8 {
                continue;
            }
            seq += 1;
            let number = format!("2026-{seq:03}");
            let issue = month * 30 + 1 + i64::try_from(rng.below(25)).unwrap_or(0);
            let gross = c
                .retainer
                .unwrap_or_else(|| i64::try_from(300_000 + rng.below(11_700) * 1_000).unwrap_or(0));
            let id = i64::from(seq);
            items.push((
                OpenItem {
                    id,
                    number: number.clone(),
                    vs: None,
                    customer: c.name.to_owned(),
                    known_accounts: Vec::new(),
                    issue_date: day_of(issue),
                    due_date: Some(day_of(issue + 14)),
                    open: czk(gross),
                },
                ci,
                issue,
            ));
            let roll = rng.below(100);
            let b = match roll {
                0..=67 => Behaviour::OnTime,
                68..=75 => Behaviour::NoSymbol,
                76..=80 => Behaviour::Typo,
                81..=84 => Behaviour::NewAccount,
                85..=88 => Behaviour::Instalments,
                89..=92 => Behaviour::Together,
                93..=95 => Behaviour::LessFee,
                _ => Behaviour::Never,
            };
            behaviours.push((id, b));
        }
    }
    let mut together_pending: Option<(i64, usize, i64, String)> = None;
    for (item, ci, issue) in &items {
        let c = &CUSTOMERS[*ci];
        let b = behaviours
            .iter()
            .find(|(id, _)| *id == item.id)
            .map(|(_, b)| *b)
            .unwrap_or(Behaviour::OnTime);
        let pay_day = issue + 14 + i64::try_from(rng.below(25)).unwrap_or(0) - 10;
        let vs = item.number.replace('-', "");
        let base = |amount: i64| BankLine {
            counterparty_name: Some(c.bank_name.to_owned()),
            counterparty_account: Some(c.account.to_owned()),
            vs: Some(vs.trim_start_matches('0').to_owned()),
            ks: Some("308".into()),
            message: Some(format!("Platba {}", c.bank_name)),
            ..blank(amount)
        };
        let gross = item.open.minor();
        match b {
            Behaviour::OnTime => push(
                day_of(pay_day),
                base(gross),
                Truth::Settles(vec![(item.id, gross)]),
            ),
            Behaviour::NoSymbol => push(
                day_of(pay_day),
                BankLine {
                    vs: None,
                    message: Some(format!("Faktura {}", item.number)),
                    ..base(gross)
                },
                Truth::Settles(vec![(item.id, gross)]),
            ),
            Behaviour::Typo => {
                let mut typo: Vec<char> = vs.chars().collect();
                let last = typo.len() - 1;
                typo[last] = if typo[last] == '9' {
                    '8'
                } else {
                    char::from(u8::try_from(typo[last] as u32 + 1).unwrap_or(b'0'))
                };
                let typo: String = typo.into_iter().collect();
                push(
                    day_of(pay_day),
                    BankLine {
                        vs: Some(typo),
                        ..base(gross)
                    },
                    Truth::Settles(vec![(item.id, gross)]),
                );
            }
            Behaviour::NewAccount => push(
                day_of(pay_day),
                BankLine {
                    counterparty_account: Some(c.other_account.to_owned()),
                    ..base(gross)
                },
                Truth::Settles(vec![(item.id, gross)]),
            ),
            Behaviour::Instalments => {
                let half = gross / 2;
                // The first part is confirmed by a person; the second then
                // pays exactly what's left.
                push(day_of(pay_day), base(half), Truth::Manual);
                push(
                    day_of(pay_day + 20),
                    base(gross - half),
                    Truth::Settles(vec![(item.id, gross - half)]),
                );
            }
            Behaviour::Together => match together_pending.take() {
                // Pays this one and the customer's earlier one in one transfer.
                Some((prev_id, prev_ci, prev_gross, prev_number)) if prev_ci == *ci => push(
                    day_of(pay_day),
                    BankLine {
                        message: Some(format!("Faktury {prev_number} a {}", item.number)),
                        ..base(gross + prev_gross)
                    },
                    Truth::Settles(vec![(prev_id, prev_gross), (item.id, gross)]),
                ),
                other => {
                    // No earlier one waiting: pay normally, and wait to pair the next.
                    if let Some((prev_id, prev_ci, prev_gross, prev_number)) = other {
                        let pc = &CUSTOMERS[prev_ci];
                        push(
                            day_of(pay_day),
                            BankLine {
                                counterparty_name: Some(pc.bank_name.to_owned()),
                                counterparty_account: Some(pc.account.to_owned()),
                                vs: Some(prev_number.replace('-', "")),
                                ..blank(prev_gross)
                            },
                            Truth::Settles(vec![(prev_id, prev_gross)]),
                        );
                    }
                    together_pending = Some((item.id, *ci, gross, item.number.clone()));
                }
            },
            Behaviour::LessFee => push(day_of(pay_day), base(gross - 1_500), Truth::Manual),
            Behaviour::Never => {}
        }
    }
    if let Some((prev_id, prev_ci, prev_gross, prev_number)) = together_pending {
        let pc = &CUSTOMERS[prev_ci];
        push(
            day_of(360),
            BankLine {
                counterparty_name: Some(pc.bank_name.to_owned()),
                counterparty_account: Some(pc.account.to_owned()),
                vs: Some(prev_number.replace('-', "")),
                ..blank(prev_gross)
            },
            Truth::Settles(vec![(prev_id, prev_gross)]),
        );
    }
    // Money out and noise.
    for month in 0..12i64 {
        push(
            day_of(month * 30 + 14),
            BankLine {
                counterparty_name: Some("Kanceláře Korunní s.r.o.".into()),
                counterparty_account: Some("2400123456/2010".into()),
                vs: Some("2026".into()),
                message: Some("Nájem".into()),
                ..blank(-1_200_000)
            },
            Truth::Rule(2),
        );
        push(
            day_of(month * 30 + 29),
            BankLine {
                message: Some("Poplatek za vedení účtu".into()),
                ..blank(-3_000)
            },
            Truth::Rule(1),
        );
        if month % 3 == 0 {
            push(
                day_of(month * 30 + 20),
                BankLine {
                    counterparty_name: Some("Pojišťovna Jistota a.s.".into()),
                    counterparty_account: Some("5050505050/0300".into()),
                    ..blank(-250_000)
                },
                if month == 0 {
                    Truth::Manual
                } else {
                    Truth::Rule(3)
                },
            );
        }
        push(
            day_of(month * 30 + 7),
            BankLine {
                message: Some("Nákup: Papírnictví".into()),
                ..blank(-i64::try_from(10_000 + rng.below(90_000)).unwrap_or(0))
            },
            Truth::Manual,
        );
    }
    for d in [40, 190, 300] {
        push(
            day_of(d),
            BankLine {
                counterparty_name: Some("Alza.cz".into()),
                message: Some("Vratka přeplatku".into()),
                ..blank(49_900)
            },
            Truth::Manual,
        );
    }
    let total = days.values().map(Vec::len).sum();
    (items.into_iter().map(|(i, _, _)| i).collect(), days, total)
}

fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: 1,
            name: "Bank fee".into(),
            when: vec![
                Condition::MessageContains("poplatek".into()),
                Condition::Direction(Direction::Out),
            ],
            then: Action::Book {
                account: "568".into(),
                vat_code: None,
                memo: "Bankovní poplatek".into(),
            },
            auto_accept: true,
        },
        Rule {
            id: 2,
            name: "Rent".into(),
            when: vec![
                Condition::CounterpartyAccount("2400123456/2010".into()),
                Condition::Direction(Direction::Out),
            ],
            then: Action::Book {
                account: "518".into(),
                vat_code: Some("IN21".into()),
                memo: "Nájem kanceláře".into(),
            },
            auto_accept: true,
        },
    ]
}

#[test]
fn a_fixture_year_is_mostly_matched_without_help_and_never_wrongly() {
    let (mut items, days, total) = fixture_year();
    let mut rules = rules();
    let policy = Policy::default();
    let (mut auto_right, mut auto_wrong, mut asked) = (0usize, Vec::new(), 0usize);
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for (date, lines) in &days {
        let raw: Vec<BankLine> = lines.iter().map(|d| d.line.clone()).collect();
        let normalised = normalise("CZ2703000000000123454412", &raw);
        for (n, day) in normalised.iter().zip(lines) {
            // The engine sees what's open on the day.
            let open: Vec<OpenItem> = items
                .iter()
                .filter(|i| i.open.minor() > 0 && i.issue_date <= *date)
                .cloned()
                .collect();
            let s = suggest(n, &open, &rules, &policy);
            for c in &s.candidates {
                assert_eq!(
                    c.score,
                    c.contributions.iter().map(|x| x.points).sum::<i32>()
                );
            }
            let proposed = match &s.proposal {
                Proposal::Settle { best } => Truth::Settles(
                    best.allocations
                        .iter()
                        .map(|(id, m)| (*id, m.minor()))
                        .collect(),
                ),
                Proposal::Rule { rule_id, .. } => Truth::Rule(*rule_id),
                Proposal::Unmatched => Truth::Manual,
            };
            if s.auto {
                if proposed == day.truth {
                    auto_right += 1;
                } else {
                    auto_wrong.push(format!(
                        "{date} {:?}: proposed {proposed:?}, truth {:?}",
                        n.line, day.truth
                    ));
                }
            } else {
                asked += 1;
                *reasons
                    .entry(
                        s.held_because
                            .clone()
                            .unwrap_or_default()
                            .split(' ')
                            .take(3)
                            .collect::<Vec<_>>()
                            .join(" "),
                    )
                    .or_default() += 1;
            }
            // Whoever decided, the books now hold the truth, and the payer's
            // account is remembered for next time.
            if let Truth::Settles(allocs) = &day.truth {
                for (id, amount) in allocs {
                    if let Some(item) = items.iter_mut().find(|i| i.id == *id) {
                        item.open = czk(item.open.minor() - amount);
                        if let Some(a) = &day.line.counterparty_account
                            && !item.known_accounts.contains(a)
                        {
                            let customer = item.customer.clone();
                            for other in items.iter_mut().filter(|i| i.customer == customer) {
                                other.known_accounts.push(a.clone());
                            }
                            break;
                        }
                    }
                }
            }
            // Booking a payment out by hand, a person makes it a rule when
            // the payee has an account to recognise it by.
            if !s.auto
                && day.truth == Truth::Manual
                && day.line.amount.minor() < 0
                && let Some(account) = &day.line.counterparty_account
            {
                let id = u32::try_from(rules.len() + 1).unwrap_or(u32::MAX);
                rules.push(Rule {
                    id,
                    name: format!("Payments to {account}"),
                    when: vec![
                        Condition::CounterpartyAccount(account.clone()),
                        Condition::Direction(Direction::Out),
                    ],
                    then: Action::Book {
                        account: "548".into(),
                        vat_code: None,
                        memo: "Pojištění".into(),
                    },
                    auto_accept: true,
                });
            }
            if day.truth == Truth::Manual
                && let Some(vs) = &day.line.vs
                && let Some(item) = items
                    .iter_mut()
                    .find(|i| i.number.replace('-', "") == *vs && i.open.minor() > 0)
            {
                // A person records the instalment or the short payment.
                item.open = czk((item.open.minor() - day.line.amount.minor()).max(0));
            }
        }
    }
    let rate = auto_right * 100 / total;
    println!(
        "{total} lines: {auto_right} accepted automatically ({rate} %), {asked} asked, {} wrong; held because {reasons:?}",
        auto_wrong.len()
    );
    assert!(
        auto_wrong.is_empty(),
        "accepted wrongly:\n{}",
        auto_wrong.join("\n")
    );
    assert!(rate >= 85, "only {rate} % accepted automatically");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn every_score_is_the_sum_of_its_contributions(
        amount in 1i64..10_000_000,
        vs in proptest::option::of(0u32..3_000_000),
        opens in prop::collection::vec((1i64..10_000_000, 0u32..3_000_000, 0usize..3), 0..8),
        day in 0i64..365,
    ) {
        let line = BankLine {
            sequence: 1,
            booking_date: day_of(day),
            value_date: None,
            amount: czk(amount),
            reversal: false,
            counterparty_name: Some("Northwind Traders".into()),
            counterparty_account: Some("19-2000145399/0800".into()),
            vs: vs.map(|v| v.to_string()),
            ks: None,
            ss: None,
            message: Some("Faktura 2026-001".into()),
            bank_ref: None,
        };
        let items: Vec<OpenItem> = opens.iter().enumerate().map(|(i, (open, number, c))| OpenItem {
            id: i64::try_from(i).unwrap_or(0),
            number: format!("2026-{number:03}"),
            vs: None,
            customer: CUSTOMERS[*c].name.into(),
            known_accounts: vec![CUSTOMERS[*c].account.into()],
            issue_date: day_of(day / 2),
            due_date: Some(day_of(day / 2 + 14)),
            open: czk(*open),
        }).collect();
        let n = normalise("CZ2703000000000123454412", &[line]);
        let s = suggest(&n[0], &items, &[], &Policy::default());
        for c in &s.candidates {
            prop_assert_eq!(c.score, c.contributions.iter().map(|x| x.points).sum::<i32>());
            let allocated: i64 = c.allocations.iter().map(|(_, m)| m.minor()).sum();
            prop_assert!(allocated <= amount, "never allocates more than the line");
        }
        if s.auto && let Proposal::Settle { best } = &s.proposal {
            prop_assert!(best.settles_exactly);
        }
    }
}

fn open(id: i64, number: &str, customer: usize, amount: i64, issued: &str) -> OpenItem {
    OpenItem {
        id,
        number: number.into(),
        vs: None,
        customer: CUSTOMERS[customer].name.into(),
        known_accounts: vec![CUSTOMERS[customer].account.into()],
        issue_date: issued.into(),
        due_date: Some(issued.into()),
        open: czk(amount),
    }
}

fn paid(amount: i64, vs: Option<&str>, message: &str) -> skyla_bank::Normalised {
    let line = BankLine {
        sequence: 1,
        booking_date: "2026-03-20".into(),
        value_date: None,
        amount: czk(amount),
        reversal: false,
        counterparty_name: Some("NORTHWIND TRADERS SRO".into()),
        counterparty_account: Some("19-2000145399/0800".into()),
        vs: vs.map(Into::into),
        ks: None,
        ss: None,
        message: Some(message.into()),
        bank_ref: None,
    };
    normalise("CZ2703000000000123454412", &[line]).remove(0)
}

#[test]
fn two_identical_retainers_without_a_symbol_are_asked_about() {
    let items = [
        open(1, "2026-010", 0, 8_470_000, "2026-02-01"),
        open(2, "2026-022", 0, 8_470_000, "2026-03-01"),
    ];
    let s = suggest(
        &paid(8_470_000, None, "Platba"),
        &items,
        &[],
        &Policy::default(),
    );
    assert!(!s.auto);
    assert!(
        s.held_because
            .as_deref()
            .is_some_and(|w| w.contains("too close")),
        "{:?}",
        s.held_because
    );
    // With the symbol it's clear.
    let s = suggest(
        &paid(8_470_000, Some("2026022"), "Platba"),
        &items,
        &[],
        &Policy::default(),
    );
    assert!(s.auto);
    let Proposal::Settle { best } = &s.proposal else {
        panic!()
    };
    assert_eq!(best.numbers, ["2026-022"]);
    let explained: Vec<_> = best
        .contributions
        .iter()
        .map(|c| (c.signal, c.points))
        .collect();
    assert_eq!(
        explained,
        [
            (skyla_bank::Signal::VsExact, 45),
            (skyla_bank::Signal::AmountExact, 30),
            (skyla_bank::Signal::KnownAccount, 20),
            (skyla_bank::Signal::NameSimilar, 15),
            (skyla_bank::Signal::DateWindow, 5),
        ]
    );
    assert_eq!(best.score, 115);
}

#[test]
fn a_split_must_name_its_invoices_to_go_through_alone() {
    let items = [
        open(1, "2026-010", 0, 2_000_000, "2026-02-01"),
        open(2, "2026-022", 0, 3_000_000, "2026-03-01"),
        open(3, "2026-030", 0, 1_000_000, "2026-03-05"),
    ];
    let named = suggest(
        &paid(5_000_000, None, "Faktury 2026-010 a 2026-022"),
        &items,
        &[],
        &Policy::default(),
    );
    assert!(named.auto, "{:?}", named.held_because);
    let Proposal::Settle { best } = &named.proposal else {
        panic!()
    };
    assert_eq!(best.numbers, ["2026-010", "2026-022"]);
    assert_eq!(best.allocations, [(1, czk(2_000_000)), (2, czk(3_000_000))]);
    // The same amount without the numbers waits for a person.
    let unnamed = suggest(
        &paid(5_000_000, None, "Platba"),
        &items,
        &[],
        &Policy::default(),
    );
    assert!(!unnamed.auto);
    assert!(
        unnamed
            .held_because
            .as_deref()
            .is_some_and(|w| w.contains("doesn't name them"))
    );
}

#[test]
fn money_out_needs_a_rule_and_rules_ask_unless_opted_in() {
    let mut rule = rules().remove(1);
    let rent = BankLine {
        counterparty_account: Some("2400123456/2010".into()),
        ..BankLine {
            amount: czk(-1_200_000),
            ..paid(1, None, "Nájem").line
        }
    };
    let n = normalise("CZ2703000000000123454412", &[rent]).remove(0);
    assert!(!suggest(&n, &[], &[], &Policy::default()).auto);
    assert!(suggest(&n, &[], std::slice::from_ref(&rule), &Policy::default()).auto);
    rule.auto_accept = false;
    let s = suggest(&n, &[], &[rule], &Policy::default());
    assert!(!s.auto && matches!(s.proposal, Proposal::Rule { rule_id: 2, .. }));
}
