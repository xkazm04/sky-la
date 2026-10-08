//! WP-17 acceptance: each format reads its samples into exact amounts,
//! dates, symbols and parties; every sample ties out (opening + lines =
//! closing); and detection picks the right parser.
//!
//! The samples are synthetic, laid out as Czech banks export each format.

use skyla_bank::{Amounts, BankLine, CsvProfile, Format, Statement, detect, parse};
use skyla_money::{Currency, Money};

fn sample(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/samples/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("sample")
}

fn czk(minor: i64) -> Money {
    Money::new(minor, Currency::CZK)
}

fn read(name: &str, csv: Option<&CsvProfile>) -> Vec<Statement> {
    let bytes = sample(name);
    let statements = parse(&bytes, csv).unwrap_or_else(|e| panic!("{name}: {e}"));
    for s in &statements {
        if let (Some(open), Some(close)) = (s.opening, s.closing) {
            let sum = s
                .lines
                .iter()
                .try_fold(open, |acc, l| acc.checked_add(l.amount))
                .expect("sum");
            assert_eq!(sum, close, "{name} ties out");
        }
    }
    statements
}

fn summary(l: &BankLine) -> (&str, i64, Option<&str>, Option<&str>) {
    (
        l.booking_date.as_str(),
        l.amount.minor(),
        l.vs.as_deref(),
        l.counterparty_name.as_deref(),
    )
}

#[test]
fn camt_053_001_02() {
    let s = read("camt053-001.02.xml", None);
    assert_eq!(s.len(), 1);
    let s = &s[0];
    assert_eq!(s.format, Format::Camt053);
    assert_eq!(s.account.iban.as_deref(), Some("CZ2703000000000123454412"));
    assert_eq!(
        (s.from.as_deref(), s.to.as_deref()),
        (Some("2026-09-01"), Some("2026-09-30"))
    );
    assert_eq!(
        (s.opening, s.closing),
        (Some(czk(83_051_000)), Some(czk(96_813_000)))
    );
    let got: Vec<_> = s.lines.iter().map(summary).collect();
    assert_eq!(
        got,
        [
            (
                "2026-09-08",
                19_360_000,
                Some("2026097"),
                Some("Acme Analytics a.s.")
            ),
            (
                "2026-09-15",
                -12_000_000,
                Some("20260915"),
                Some("Kanceláře Korunní s.r.o.")
            ),
            (
                "2026-09-29",
                6_402_000,
                Some("2026114"),
                Some("Northwind Traders s.r.o.")
            ),
        ]
    );
    assert_eq!(s.lines[0].ks.as_deref(), Some("308"));
    assert_eq!(
        s.lines[0].counterparty_account.as_deref(),
        Some("CZ5501000000000987654321")
    );
    assert_eq!(
        s.lines[1].counterparty_account.as_deref(),
        Some("2400123456/2010")
    );
    assert_eq!(s.lines[1].message.as_deref(), Some("Nájem září"));
    assert_eq!(s.lines[2].bank_ref.as_deref(), Some("CSOB-77811902"));
    assert_eq!(s.warnings, ["entry 4 is PDNG (not booked) and was skipped"]);
}

#[test]
fn camt_053_001_08_reads_party_names_status_codes_and_reversals() {
    let s = &read("camt053-001.08.xml", None)[0];
    assert_eq!(s.lines.len(), 2);
    assert_eq!(
        summary(&s.lines[0]),
        ("2026-10-01", -29_950, None, Some("Hosting Praha"))
    );
    assert!(s.lines[1].reversal);
    assert_eq!(s.lines[1].amount, czk(-100));
}

#[test]
fn mt940_with_subfields() {
    let s = read("mt940-subfields.sta", None);
    let s = &s[0];
    assert_eq!(s.format, Format::Mt940);
    assert_eq!(s.account.iban.as_deref(), Some("CZ2703000000000123454412"));
    assert_eq!(s.number.as_deref(), Some("9/1"));
    let got: Vec<_> = s.lines.iter().map(summary).collect();
    assert_eq!(
        got,
        [
            (
                "2026-09-08",
                19_360_000,
                Some("2026097"),
                Some("Acme Analytics a.s.")
            ),
            ("2026-09-15", -12_000_000, None, Some("Kancelare Korunni")),
            (
                "2026-09-29",
                6_402_000,
                Some("2026114"),
                Some("Northwind Traders s.r.o.")
            ),
        ]
    );
    assert_eq!(
        s.lines[0].counterparty_account.as_deref(),
        Some("0000000987654321/0100")
    );
    assert_eq!(s.lines[0].bank_ref.as_deref(), Some("77810021"));
    assert_eq!(s.lines[1].bank_ref.as_deref(), Some("77810455"));
    assert_eq!(
        s.lines[0].message.as_deref(),
        Some("Faktura 2026-097VS2026097")
    );
}

#[test]
fn mt940_in_a_swift_envelope_with_two_statements() {
    let s = read("mt940-envelope.sta", None);
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].account.number.as_deref(), Some("2000145399"));
    assert_eq!(s[0].account.bank_code.as_deref(), Some("0800"));
    let l = &s[0].lines;
    assert_eq!(
        summary(&l[0]),
        (
            "2026-10-02",
            250_050,
            Some("2026115"),
            Some("Studio Brno s.r.o.")
        )
    );
    assert_eq!(l[0].ks.as_deref(), Some("308"));
    assert_eq!(l[0].message.as_deref(), Some("Zaloha"));
    assert!(l[1].reversal, "RC: a reversed credit");
    assert_eq!(l[1].amount, czk(-10_000));
    assert_eq!(s[1].lines[0].amount, czk(-40_050));
    assert_eq!(
        s[1].lines[0].message.as_deref(),
        Some("Poplatek za vedeni uctu")
    );
}

#[test]
fn abo_gpc_in_windows_1250() {
    let s = read("gpc-2026-09.gpc", None);
    let s = &s[0];
    assert_eq!(s.format, Format::Gpc);
    assert_eq!(s.account.number.as_deref(), Some("123454412"));
    assert_eq!(
        (s.from.as_deref(), s.to.as_deref(), s.number.as_deref()),
        (Some("2026-08-31"), Some("2026-09-30"), Some("9"))
    );
    let got: Vec<_> = s.lines.iter().map(summary).collect();
    assert_eq!(
        got,
        [
            (
                "2026-09-08",
                19_360_000,
                Some("2026097"),
                Some("Acme Analytics")
            ),
            (
                "2026-09-29",
                6_402_000,
                Some("2026114"),
                Some("Northwind Traders")
            ),
            ("2026-09-15", -12_000_000, None, Some("Nájem září")),
        ]
    );
    assert_eq!(
        s.lines[0].counterparty_account.as_deref(),
        Some("987654321/0100")
    );
    assert_eq!(
        s.lines[1].counterparty_account.as_deref(),
        Some("19-2000145399/0800")
    );
    assert_eq!(s.lines[0].ks.as_deref(), Some("308"));
    assert_eq!(s.lines[2].counterparty_account, None);
    assert_eq!(
        s.lines[2].message.as_deref(),
        Some("Platba nájemného za září, kancelář Korunní")
    );
}

#[test]
fn fio_csv_with_its_preamble() {
    let s = read("fio.csv", Some(&CsvProfile::fio()));
    let s = &s[0];
    assert_eq!(s.format, Format::Csv);
    assert_eq!(s.account.iban.as_deref(), Some("CZ6520100000002000145399"));
    assert_eq!(
        (s.from.as_deref(), s.to.as_deref()),
        (Some("2026-10-01"), Some("2026-10-07"))
    );
    let got: Vec<_> = s.lines.iter().map(summary).collect();
    assert_eq!(
        got,
        [
            (
                "2026-10-02",
                302_500,
                Some("2026116"),
                Some("Acme Analytics a.s.")
            ),
            ("2026-10-05", -500_000, None, Some("Kanceláře Korunní")),
            ("2026-10-07", -57_500, None, None),
        ]
    );
    assert_eq!(
        s.lines[0].counterparty_account.as_deref(),
        Some("987654321/0100")
    );
    assert_eq!(
        s.lines[1].message.as_deref(),
        Some("Záloha energie Energie; říjen")
    );
    assert_eq!(s.lines[0].bank_ref.as_deref(), Some("26001"));
}

#[test]
fn a_debit_credit_csv_profile() {
    let profile = CsvProfile {
        name: "Generic".into(),
        delimiter: ',',
        decimal: '.',
        date: "Date".into(),
        amounts: Amounts::DebitCredit {
            debit: "Debit".into(),
            credit: "Credit".into(),
        },
        currency: None,
        default_currency: "CZK".into(),
        value_date: None,
        counterparty_name: None,
        counterparty_account: None,
        counterparty_bank: None,
        vs: None,
        ks: None,
        ss: None,
        message: vec!["Description".into()],
        bank_ref: Some("Reference".into()),
        opening_key: None,
        closing_key: None,
    };
    let s = &read("debit-credit.csv", Some(&profile))[0];
    assert_eq!(
        s.lines.iter().map(|l| l.amount.minor()).collect::<Vec<_>>(),
        [8_470_000, -3_000]
    );
    assert_eq!(
        s.lines[0].vs.as_deref(),
        Some("2026113"),
        "VS found in the description"
    );
    assert_eq!(s.warnings.len(), 1, "no closing balance to tie out");
}

#[test]
fn detection_picks_each_format() {
    assert_eq!(detect(&sample("camt053-001.02.xml")), Format::Camt053);
    assert_eq!(detect(&sample("mt940-subfields.sta")), Format::Mt940);
    assert_eq!(detect(&sample("mt940-envelope.sta")), Format::Mt940);
    assert_eq!(detect(&sample("gpc-2026-09.gpc")), Format::Gpc);
    assert_eq!(detect(&sample("fio.csv")), Format::Csv);
    assert!(
        parse(&sample("fio.csv"), None).is_err(),
        "CSV needs a profile"
    );
}

#[test]
fn damaged_files_fail_with_a_reason() {
    let camt = String::from_utf8(sample("camt053-001.02.xml")).expect("utf8");
    let broken = camt.replace(
        "<CdtDbtInd>CRDT</CdtDbtInd><RvslInd>false</RvslInd>",
        "<RvslInd>false</RvslInd>",
    );
    let err = parse(broken.as_bytes(), None).expect_err("no indicator");
    assert!(err.to_string().contains("credit/debit"), "{err}");
    let dtd = "<?xml version=\"1.0\"?><!DOCTYPE x [<!ENTITY a \"aaaa\">]><Document xmlns=\"urn:iso:std:iso:20022:tech:xsd:camt.053.001.02\"><BkToCstmrStmt/></Document>";
    assert!(parse(dtd.as_bytes(), None).is_err(), "DTDs are refused");
    let gpc = sample("gpc-2026-09.gpc");
    let mut bad = gpc.clone();
    // Posting code 9 doesn't exist.
    let pos = 128 + 2 + 60;
    bad[pos] = b'9';
    let err = parse(&bad, None).expect_err("bad code");
    assert!(err.to_string().contains("line 2"), "{err}");
    assert!(matches!(
        parse(&vec![b' '; skyla_bank::MAX_BYTES + 1], None),
        Err(skyla_bank::BankError::TooLarge { .. })
    ));
}
