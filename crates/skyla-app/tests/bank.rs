//! WP-19 acceptance (the core half): import → accept certain matches →
//! split one → create a rule, against the real ledger.

#![allow(clippy::unwrap_used)]

use skyla_app::Core;
use skyla_app::dto::{BankAllocationDto, BankRuleInputDto};

const SECOND: &str = include_str!("../../../packages/fixtures/data/statements/csob-2026-10-07.xml");

fn b64(text: &str) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
}

fn status(core: &Core, id: &str) -> String {
    core.bank_statement()
        .unwrap()
        .lines
        .into_iter()
        .find(|l| l.id == id)
        .map(|l| l.status)
        .unwrap()
}

#[test]
fn import_accept_split_and_rule() {
    let core = Core::demo().unwrap();
    // The first statement: Pixelfarm's payment settles its received invoice.
    let first = core.bank_statement().unwrap();
    let pixelfarm = first.lines.iter().find(|l| l.id == "s1-1").unwrap();
    assert_eq!(
        (pixelfarm.status.as_str(), pixelfarm.proposal.as_deref()),
        ("certain", Some("Pay PF-2026-0917"))
    );
    assert!(
        first
            .lines
            .iter()
            .filter(|l| l.id != "s1-1")
            .all(|l| l.status == "needs_you")
    );

    // A statement with a line missing is refused, loudly.
    let missing = SECOND.replacen(
        "<Amt Ccy=\"CZK\">12000.00</Amt>",
        "<Amt Ccy=\"CZK\">0.00</Amt>",
        1,
    );
    let err = core
        .import_bank_statement("broken.xml", &b64(&missing))
        .unwrap_err();
    assert!(err.to_string().contains("doesn't tie out"), "{err}");

    let imported = core
        .import_bank_statement("csob-2026-10-07.xml", &b64(SECOND))
        .unwrap();
    assert_eq!(imported.imports.len(), 2);
    assert_eq!(imported.closing.minor, 88_590_158);
    assert_eq!(
        status(&core, "s2-1"),
        "certain",
        "Studio Brno pays what's open on 2026-102"
    );
    let again = core
        .import_bank_statement("csob-2026-10-07.xml", &b64(SECOND))
        .unwrap_err();
    assert!(again.to_string().contains("imported before"), "{again}");
    let inbox = core.proposals().unwrap();
    assert!(
        inbox.iter().any(|p| p.id == "bank-s2-2"),
        "the rent needs you"
    );

    let accepted = core.accept_certain_bank_lines().unwrap();
    let booked: Vec<_> = accepted
        .lines
        .iter()
        .filter(|l| l.status == "booked")
        .map(|l| (l.id.as_str(), l.booked_as.as_deref()))
        .collect();
    assert_eq!(
        booked,
        [
            ("s2-1", Some("Settles 2026-102")),
            ("s1-1", Some("Pays PF-2026-0917"))
        ]
    );
    let invoice = core
        .invoices()
        .unwrap()
        .into_iter()
        .find(|i| i.number.as_deref() == Some("2026-102"))
        .unwrap();
    assert_eq!((invoice.status.as_str(), invoice.open.minor), ("paid", 0));

    // Split the Datart purchase: monitor and the extended warranty, VAT from the gross.
    let row = |account: &str, amount: &str| BankAllocationDto {
        entry_id: None,
        account: Some(account.into()),
        vat_code: Some("IN21".into()),
        amount: amount.into(),
    };
    let wrong = core
        .book_bank_line("s2-3", &[row("501", "3 630,00"), row("518", "1 000,00")])
        .unwrap_err();
    assert!(
        wrong.to_string().contains("the line is 4\u{a0}840,00"),
        "{wrong}"
    );
    let split = core
        .book_bank_line("s2-3", &[row("501", "3 630,00"), row("518", "1 210,00")])
        .unwrap();
    let datart = split.lines.iter().find(|l| l.id == "s2-3").unwrap();
    assert_eq!(datart.booked_as.as_deref(), Some("Split across 501 + 518"));
    let entry = core
        .journal("2026-10-07", "2026-10-07")
        .unwrap()
        .into_iter()
        .find(|e| Some(e.id) == datart.entry_id)
        .unwrap();
    let lines: Vec<(&str, i64)> = entry
        .lines
        .iter()
        .map(|l| (l.account.as_str(), l.amount.minor))
        .collect();
    assert_eq!(
        lines,
        [
            ("501", 300_000),
            ("343", 63_000),
            ("518", 100_000),
            ("343", 21_000),
            ("221", -484_000)
        ]
    );
    assert_eq!(entry.approved_by.as_deref(), Some("user"));

    // A rule from the rent line books it, and stays for next month.
    let ruled = core
        .create_bank_rule(
            "s2-2",
            &BankRuleInputDto {
                name: "Office rent".into(),
                account: "518".into(),
                vat_code: None,
                auto_accept: true,
            },
        )
        .unwrap();
    assert_eq!(ruled.rules.len(), 1);
    assert_eq!(
        ruled.rules[0].summary,
        "to or from 2400123456/2010, money out → book to 518"
    );
    assert_eq!(status(&core, "s2-2"), "booked");
    assert!(core.integrity().unwrap().chain_intact);
    let tb = core.trial_balance(None, "2026-10-07").unwrap();
    assert_eq!(tb.total_debit, tb.total_credit);
    assert!(
        !core
            .proposals()
            .unwrap()
            .iter()
            .any(|p| p.id.starts_with("bank-s2")),
        "nothing left in the inbox from the import"
    );
}
