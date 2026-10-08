//! WP-26 acceptance: IBANs and personal IDs never appear in any payload,
//! whatever the scope and wherever they hide (property tests), and the
//! register replays byte-identically and shows tampering.

#![allow(clippy::unwrap_used)]

use proptest::prelude::*;
use rusqlite::Connection;
use serde_json::{Value, json};
use skyla_egress::register::{self, NewRun};
use skyla_egress::{FieldClass, Gate, Pseudonyms, Role};

/// A valid IBAN for `country` with `bban` (mod-97 check digits).
fn iban(country: &str, bban: &str) -> String {
    let rearranged = format!("{bban}{country}00");
    let mut rem: u32 = 0;
    for c in rearranged.chars() {
        let v = c.to_digit(36).unwrap();
        for d in v.to_string().chars() {
            rem = (rem * 10 + d.to_digit(10).unwrap()) % 97;
        }
    }
    format!("{country}{:02}{bban}", 98 - rem)
}

fn spaced(s: &str) -> String {
    s.chars()
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

fn luhn_complete(prefix: &str) -> String {
    for d in 0..10 {
        let candidate = format!("{prefix}{d}");
        let sum: u32 = candidate
            .chars()
            .rev()
            .enumerate()
            .map(|(i, c)| {
                let v = c.to_digit(10).unwrap();
                if i % 2 == 1 {
                    if v * 2 > 9 { v * 2 - 9 } else { v * 2 }
                } else {
                    v
                }
            })
            .sum();
        if sum.is_multiple_of(10) {
            return candidate;
        }
    }
    unreachable!()
}

/// Something that must never leave, in one of its written forms, and the
/// digits that identify it.
fn secret() -> impl Strategy<Value = (String, String)> {
    prop_oneof![
        // IBANs: Czech, Slovak, German; printed with spaces or run together.
        (
            prop::sample::select(vec![("CZ", 20), ("SK", 20), ("DE", 18)]),
            "[0-9]{20}",
            any::<bool>()
        )
            .prop_map(|((cc, len), digits, space)| {
                let i = iban(cc, &digits[..len]);
                let key = i[4..].to_owned();
                (if space { spaced(&i) } else { i }, key)
            }),
        // Personal IDs (rodné číslo), with or without the slash.
        (
            0..100_u32,
            prop::sample::select(vec![0_u32, 20, 50, 70]),
            1..=12_u32,
            1..=28_u32,
            0..10_000_u32,
            any::<bool>()
        )
            .prop_map(|(yy, plus, mm, dd, tail, slash)| {
                let head = format!("{yy:02}{:02}{dd:02}", mm + plus);
                let tail = format!("{tail:04}");
                let shown = if slash {
                    format!("{head}/{tail}")
                } else {
                    format!("{head}{tail}")
                };
                (shown, format!("{head}{tail}"))
            }),
        // Domestic accounts.
        ("[1-9][0-9]{5}", "[1-9][0-9]{9}", "[0-9]{4}")
            .prop_map(|(p, n, b)| (format!("{p}-{n}/{b}"), n)),
        // Card numbers.
        "[3-6][0-9]{14}".prop_map(|p| {
            let c = luhn_complete(&p);
            (spaced(&c), c)
        }),
    ]
}

fn scope() -> impl Strategy<Value = Vec<FieldClass>> {
    prop::sample::subsequence(
        vec![
            FieldClass::Aggregates,
            FieldClass::AccountTotals,
            FieldClass::CounterpartyNames,
            FieldClass::LineMemos,
            FieldClass::Documents,
        ],
        0..=5,
    )
}

/// Hides `secret` somewhere in a tool-result-shaped value.
fn wrap(secret: &str, filler: &str, place: u8) -> Value {
    match place % 5 {
        0 => json!({ "reference": format!("{filler} {secret}") }),
        1 => {
            json!({ "lines": [{ "memo": "x" }, { "counterparty": format!("{secret} {filler}") }] })
        }
        2 => json!({ "note": { "deep": [[format!("{filler}:{secret}")]] } }),
        3 => json!({ format!("key {secret}"): 1 }),
        _ => json!([filler, secret, { "amount": { "minor": 100, "currency": "CZK" } }]),
    }
}

fn digits(s: &str) -> String {
    s.chars().filter(char::is_ascii_alphanumeric).collect()
}

fn gate(scope: Vec<FieldClass>) -> Gate {
    Gate::new(
        scope,
        Pseudonyms::new(&[("Pixelfarm s.r.o.".into(), Role::Vendor)]),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn no_identifier_survives_a_tool_result((shown, key) in secret(), filler in "[a-zA-Z ,.]{0,30}", place in any::<u8>(), scope in scope()) {
        let (out, _) = gate(scope).json(&wrap(&shown, &filler, place));
        let text = out.to_string();
        prop_assert!(!digits(&text).contains(&key), "{shown} survived as {text}");
    }

    #[test]
    fn no_identifier_survives_a_prompt((shown, key) in secret(), before in "[a-zA-ZčřžČŘŽ:.,( ]{0,20}", after in "[a-zA-ZčřžČŘŽ:.,) ]{0,20}", scope in scope()) {
        // Glued to the words around it, without spaces.
        let (out, report) = gate(scope).text(&format!("{before}{shown}{after}"));
        prop_assert!(!digits(&out).contains(&key), "{shown} survived as {out}");
        prop_assert!(!report.withheld.is_empty());
    }
}

fn run(uid: &str, payload: Vec<u8>) -> NewRun {
    NewRun {
        uid: uid.into(),
        at: "2026-10-07T09:00:00Z".into(),
        task: "tax.scenarios".into(),
        advisor: "Tax advisor".into(),
        purpose: "Flat-rate scenario".into(),
        provider: "claude-code-cli".into(),
        model: "default".into(),
        scopes: vec!["aggregates".into()],
        withheld: vec!["IBAN ×1".into()],
        tool_calls: 2,
        payload,
        outcome: "Scenario drafted".into(),
        cost_note: Some("about $0.002".into()),
    }
}

#[test]
fn the_register_replays_byte_identically_and_refuses_edits() {
    let conn = Connection::open_in_memory().unwrap();
    register::apply_schema(&conn).unwrap();
    register::apply_schema(&conn).unwrap();
    // Not even valid UTF-8 is changed.
    let payload: Vec<u8> = (0..=255_u8)
        .chain(b"{\"prompt\":\"\xc5\xa1\"}".iter().copied())
        .collect();
    let a = register::record(&conn, &run("a", payload.clone())).unwrap();
    let b = register::record(&conn, &run("b", b"second".to_vec())).unwrap();
    assert_eq!(register::replay(&conn, a).unwrap(), payload);
    assert_eq!(register::replay(&conn, b).unwrap(), b"second");
    assert_eq!(register::verify(&conn).unwrap(), None);
    let listed = register::list(&conn).unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].run.withheld, ["IBAN ×1"]);
    for sql in [
        "UPDATE egress_run SET outcome = 'x'",
        "DELETE FROM egress_run",
    ] {
        let err = conn.execute_batch(sql).unwrap_err().to_string();
        assert!(err.contains("append-only"), "{err}");
    }
    // Someone with the key who drops the guard and edits a payload is caught.
    conn.execute_batch(
        "DROP TRIGGER egress_run_no_update; UPDATE egress_run SET payload = X'00' WHERE uid = 'a';",
    )
    .unwrap();
    assert_eq!(register::verify(&conn).unwrap(), Some(a));
}
