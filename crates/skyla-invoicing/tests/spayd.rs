//! SPAYD (QR Platba) against the examples in the Czech Banking Association
//! specification, plus IBAN, IČO and supplier-profile checks.

use skyla_invoicing::spayd::{PaymentRequest, normalize_iban, parse_spayd, spayd, variable_symbol};
use skyla_invoicing::{Supplier, set_supplier, supplier, valid_ico};
use skyla_money::{Currency, Money};

fn czk(minor: i64) -> Money {
    Money::new(minor, Currency::from_code("CZK").expect("ok"))
}

fn request(iban: &str) -> PaymentRequest {
    PaymentRequest {
        iban: iban.to_owned(),
        bic: None,
        amount: czk(48_050),
        due_date: None,
        recipient: None,
        message: None,
        variable_symbol: None,
    }
}

#[test]
fn matches_the_specification_examples() {
    // Specification §"Příklady": the minimal payment.
    let minimal = PaymentRequest {
        message: Some("PLATBA ZA ZBOZI".into()),
        ..request("CZ5855000000001265098001")
    };
    assert_eq!(
        spayd(&minimal).expect("ok"),
        "SPD*1.0*ACC:CZ5855000000001265098001*AM:480.50*CC:CZK*MSG:PLATBA ZA ZBOZI"
    );
    // The same with the BIC after the IBAN.
    let with_bic = PaymentRequest {
        bic: Some("RZBCCZPP".into()),
        ..minimal.clone()
    };
    assert_eq!(
        spayd(&with_bic).expect("ok"),
        "SPD*1.0*ACC:CZ5855000000001265098001+RZBCCZPP*AM:480.50*CC:CZK*MSG:PLATBA ZA ZBOZI"
    );
    // Due date, recipient and variable symbol.
    let full = PaymentRequest {
        due_date: Some("2012-05-24".into()),
        recipient: Some("PETR DVORAK".into()),
        variable_symbol: Some("1234567890".into()),
        ..with_bic
    };
    assert_eq!(
        spayd(&full).expect("ok"),
        "SPD*1.0*ACC:CZ5855000000001265098001+RZBCCZPP*AM:480.50*CC:CZK*DT:20120524*RN:PETR DVORAK*MSG:PLATBA ZA ZBOZI*X-VS:1234567890"
    );
}

#[test]
fn escapes_asterisks_and_cuts_long_text() {
    let req = PaymentRequest {
        message: Some(format!("50% *sleva* {}", "x".repeat(80))),
        recipient: Some("Novák * Partneři".into()),
        ..request("CZ58 5500 0000 0012 6509 8001")
    };
    let s = spayd(&req).expect("ok");
    assert!(s.contains("*MSG:50%25 %2Asleva%2A x"), "{s}");
    assert!(s.contains("*RN:Novák %2A Partneři"), "{s}");
    let fields = parse_spayd(&s).expect("ok");
    let msg = &fields.iter().find(|(k, _)| k == "MSG").expect("ok").1;
    assert_eq!(msg.chars().count(), 60);
    assert!(msg.starts_with("50% *sleva* "));
    assert_eq!(fields[0], ("ACC".into(), "CZ5855000000001265098001".into()));
}

#[test]
fn refuses_what_a_bank_would_reject() {
    let err = |r: PaymentRequest| spayd(&r).expect_err("refused").to_string();
    assert!(err(request("CZ5855000000001265098002")).contains("check digits"));
    assert!(err(request("CZ58550000000012650980")).contains("24 characters"));
    assert!(
        err(PaymentRequest {
            amount: czk(0),
            ..request("CZ5855000000001265098001")
        })
        .contains("positive")
    );
    let several = err(PaymentRequest {
        variable_symbol: Some("12345678901".into()),
        due_date: Some("24.5.2012".into()),
        ..request("CZ5855000000001265098001")
    });
    assert!(
        several.contains("variable symbol") && several.contains("due date"),
        "{several}"
    );
}

#[test]
fn validates_ibans_from_several_countries() {
    for iban in [
        "CZ65 0800 0000 1920 0014 5399",
        "DE89 3704 0044 0532 0130 00",
        "GB82 WEST 1234 5698 7654 32",
        "SK31 1200 0000 1987 4263 7541",
    ] {
        let n = normalize_iban(iban).expect("ok");
        assert!(!n.contains(' '));
    }
    assert!(normalize_iban("GB82 WEST 1234 5698 7654 33").is_err());
    assert!(normalize_iban("82GB WEST 1234 5698 7654 32").is_err());
}

#[test]
fn derives_the_variable_symbol_from_the_number() {
    assert_eq!(variable_symbol("2026-001").as_deref(), Some("2026001"));
    assert_eq!(
        variable_symbol("FV/2026/00042").as_deref(),
        Some("202600042")
    );
    assert_eq!(
        variable_symbol("2026-12345678").as_deref(),
        Some("2612345678")
    );
    assert_eq!(variable_symbol("ABC"), None);
}

#[test]
fn checks_ico_and_stores_the_profile() {
    assert!(valid_ico("25596641"));
    assert!(valid_ico("27074358"));
    assert!(!valid_ico("25596642"));
    assert!(!valid_ico("2559664"));

    let conn = rusqlite::Connection::open_in_memory().expect("ok");
    skyla_ledger::apply_schema(&conn).expect("ok");
    skyla_invoicing::apply_schema(&conn).expect("ok");
    assert_eq!(supplier(&conn).expect("ok"), None);

    let mut profile = Supplier {
        name: "  Jana Nováková ".into(),
        ico: Some("25596642".into()),
        dic: None,
        address: String::new(),
        iban: Some("CZ5855000000001265098002".into()),
        bic: None,
        email: None,
        vat_payer: true,
        registration: String::new(),
    };
    let problems = set_supplier(&conn, &profile)
        .expect_err("refused")
        .to_string();
    for expected in ["address", "IČO", "DIČ", "check digits"] {
        assert!(problems.contains(expected), "{problems}");
    }

    profile.ico = Some("25596641".into());
    profile.dic = Some("cz 25596641".into());
    profile.address = "Dlouhá 12\n110 00 Praha 1".into();
    profile.iban = Some("cz58 5500 0000 0012 6509 8001".into());
    let saved = set_supplier(&conn, &profile).expect("ok");
    assert_eq!(saved.name, "Jana Nováková");
    assert_eq!(saved.dic.as_deref(), Some("CZ25596641"));
    assert_eq!(saved.iban.as_deref(), Some("CZ5855000000001265098001"));
    assert_eq!(supplier(&conn).expect("ok"), Some(saved));
}
