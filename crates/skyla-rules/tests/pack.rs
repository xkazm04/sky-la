//! WP-20 (pack data): the CZ 2026 pack loads, every value is cited, lookups
//! are effective-dated, and bad packs are refused with every problem listed.

use skyla_money::{Currency, Money, RoundingMode};
use skyla_rules::{CZ_2026, Pack, Review, RulesError};

#[test]
fn the_cz_2026_pack_loads_and_every_value_is_cited() {
    let pack = Pack::cz_2026().unwrap();
    assert_eq!(pack.provenance(), "cz-2026@2026.1");
    assert_eq!(pack.info.review, Review::Draft);
    for key in pack.keys() {
        let v = pack
            .value(key, "2026-06-30")
            .or_else(|_| pack.value(key, "2023-06-30"))
            .unwrap();
        let citation = pack.citation(&v.cite);
        assert!(
            citation.contains("Sb.") || citation.contains("Pokyny"),
            "{key}: {citation}"
        );
        assert!(
            citation.contains('§') || citation.contains("ř."),
            "{key}: {citation}"
        );
    }
    for code in &pack.vat_codes {
        assert!(
            pack.vat_rate(&code.code, "2026-03-01").is_ok(),
            "{}",
            code.code
        );
    }
}

#[test]
fn values_are_looked_up_on_a_date() {
    let pack = Pack::cz_2026().unwrap();
    assert_eq!(
        pack.percent("vat.rate.standard", "2026-09-30")
            .unwrap()
            .to_string(),
        "21"
    );
    // The 2024 consolidation: 15 % until the end of 2023, 12 % after.
    assert_eq!(
        pack.percent("vat.rate.reduced", "2023-12-31")
            .unwrap()
            .to_string(),
        "15"
    );
    assert_eq!(
        pack.percent("vat.rate.reduced", "2024-01-01")
            .unwrap()
            .to_string(),
        "12"
    );
    assert_eq!(
        pack.vat_rate("RC21S", "2026-09-03").unwrap().to_string(),
        "21"
    );
    assert!(matches!(
        pack.percent("vat.rate.reduced", "2014-06-01"),
        Err(RulesError::Missing { .. })
    ));

    assert_eq!(
        pack.amount("income_tax.flat_rate.trade.cap", "2026-01-01")
            .unwrap(),
        Money::new(120_000_000, Currency::CZK)
    );
    assert_eq!(
        pack.amount("assets.tangible.threshold", "2026-01-01")
            .unwrap(),
        Money::new(8_000_000, Currency::CZK)
    );
    assert_eq!(
        pack.days("vat.return.due_days_after_period", "2026-10-01")
            .unwrap(),
        25
    );
    assert_eq!(
        pack.rounding("vat.rounding.document", "2026-10-01")
            .unwrap(),
        RoundingMode::HalfUp
    );
    assert!(
        pack.flag("deadline.shift_to_next_working_day", "2026-10-01")
            .unwrap()
    );
    assert!(matches!(
        pack.percent("assets.tangible.threshold", "2026-01-01"),
        Err(RulesError::WrongKind { .. })
    ));
}

#[test]
fn a_bad_pack_is_refused_with_every_problem_listed() {
    let broken = CZ_2026
        .replace(
            "value = \"21\"\neffective_from = \"2013-01-01\"",
            "value = \"twenty-one\"\neffective_from = \"2013-13-01\"",
        )
        .replace(
            "cite = { act = \"zdp\", section = \"§ 16 odst. 1 písm. a)\" }",
            "cite = { act = \"nope\", section = \"\" }",
        )
        .replace(
            "effective_to = \"2023-12-31\"",
            "effective_to = \"2024-06-30\"",
        )
        .replace(
            "rate = \"vat.rate.reduced\"\ncite = { act = \"dphdp3\", section = \"ř. 41\" }",
            "rate = \"vat.rate.missing\"\ncite = { act = \"dphdp3\", section = \"ř. 41\" }",
        );
    let Err(RulesError::Invalid(problems)) = Pack::from_toml(&broken) else {
        panic!("the broken pack loaded");
    };
    let all = problems.join("\n");
    assert!(
        all.contains("\"twenty-one\" isn't a valid percent"),
        "{all}"
    );
    assert!(
        all.contains("effective_from \"2013-13-01\" isn't a date"),
        "{all}"
    );
    assert!(all.contains("cites unknown act nope"), "{all}");
    assert!(all.contains("citation has no section"), "{all}");
    assert!(
        all.contains("vat.rate.reduced: periods from 2015-01-01 and 2024-01-01 overlap"),
        "{all}"
    );
    assert!(
        all.contains("VAT code IN12: rate vat.rate.missing isn't a percent value"),
        "{all}"
    );
    assert!(problems.len() >= 6);

    assert!(matches!(
        Pack::from_toml("not = [toml"),
        Err(RulesError::Format(_))
    ));
    let unknown_field = CZ_2026.replacen("review = \"draft\"", "review = \"draft\"\nsecret = 1", 1);
    assert!(matches!(
        Pack::from_toml(&unknown_field),
        Err(RulesError::Format(_))
    ));
}

#[test]
fn deadlines_move_past_weekends_and_public_holidays() {
    use skyla_rules::date;
    let pack = Pack::cz_2026().unwrap();
    // Easter 2026 is 5 April: Good Friday 3 April, Easter Monday 6 April.
    assert_eq!(date::format(date::easter(2026)), "2026-04-05");
    assert_eq!(date::format(date::easter(2025)), "2025-04-20");
    assert_eq!(pack.holiday("2026-04-03").unwrap().name, "Velký pátek");
    assert_eq!(
        pack.holiday("2026-04-06").unwrap().name,
        "Velikonoční pondělí"
    );
    assert!(pack.holiday("2026-10-28").is_some());
    assert!(!pack.is_working_day("2026-10-25")); // a Sunday
    assert!(pack.is_working_day("2026-10-26"));

    let due = |end: &str| {
        pack.deadline_after("vat.return.due_days_after_period", end)
            .unwrap()
    };
    // September 2026: 25 October is a Sunday, so Monday 26 October.
    assert_eq!(due("2026-09-30"), "2026-10-26");
    // August 2026: 25 September is a Friday.
    assert_eq!(due("2026-08-31"), "2026-09-25");
    // February 2026: 25 March is a Wednesday.
    assert_eq!(due("2026-02-28"), "2026-03-25");
    // November 2026: 25 December is a holiday, then the 26th, then a Sunday.
    assert_eq!(due("2026-11-30"), "2026-12-28");
    // March 2026: 25 April is a Saturday.
    assert_eq!(due("2026-03-31"), "2026-04-27");
}

#[test]
fn the_calendar_round_trips_every_day_for_three_centuries() {
    use skyla_rules::date;
    for days in date::to_days(1900, 1, 1)..date::to_days(2200, 1, 1) {
        assert_eq!(date::parse(&date::format(days)), Some(days));
    }
    assert_eq!(date::weekday(date::parse("2026-10-07").unwrap()), 2); // a Wednesday
    assert_eq!(date::parse("2026-02-29"), None);
    assert_eq!(
        date::parse("2024-02-29").map(date::format).as_deref(),
        Some("2024-02-29")
    );
}

#[test]
fn the_eu_supply_codes_are_cited_and_feed_the_vat_return_rows() {
    let pack = Pack::cz_2026().unwrap();
    for (code, row, sh, category) in [("EUSVC", "21", "3", "AE"), ("EUGDS", "20", "0", "K")] {
        let c = pack.vat_code(code).unwrap();
        // No Czech VAT: the base alone feeds one row, credit-positive like a sale.
        assert_eq!(
            pack.vat_rate(code, "2026-06-30").unwrap().to_string(),
            "0",
            "{code}"
        );
        assert_eq!(c.rows.len(), 1, "{code}");
        assert_eq!(
            (c.rows[0].row.as_str(), c.rows[0].credit_positive),
            (row, true)
        );
        let eu = c.eu_supply.as_ref().unwrap();
        assert_eq!(eu.sh_code, sh, "{code}");
        assert!(
            pack.citation(&eu.cite).contains("Pokyny"),
            "{}",
            pack.citation(&eu.cite)
        );
        assert_eq!(c.einvoice.as_ref().unwrap().category, category, "{code}");
        let note = c.invoice_note.as_ref().unwrap();
        assert!(!note.cs.is_empty() && !note.en.is_empty());
        assert!(!c.outside_vat);
    }
    // Domestic codes are not EU supplies.
    assert!(pack.vat_code("OUT21").unwrap().eu_supply.is_none());
}

#[test]
fn a_bad_eu_supply_code_is_refused() {
    let broken = CZ_2026
        .replace("{ sh_code = \"3\",", "{ sh_code = \"7\",")
        .replace(
            "einvoice = { category = \"K\", exemption_reason",
            "einvoice = { category = \"S\", exemption_reason",
        );
    let Err(RulesError::Invalid(problems)) = Pack::from_toml(&broken) else {
        panic!("the broken pack loaded");
    };
    let all = problems.join("\n");
    assert!(
        all.contains("VAT code EUSVC: souhrnné hlášení code \"7\" isn't 0, 1, 2 or 3"),
        "{all}"
    );
    assert!(
        all.contains("VAT code EUGDS: an EU supply is e-invoice category K or AE"),
        "{all}"
    );

    // A blank Czech text, the original line turned into a comment.
    let no_note = CZ_2026.replace(
        "invoice_note = { cs = \"Služba",
        "invoice_note = { cs = \" \", en = \"Service\" }\n# Služba",
    );
    let Err(RulesError::Invalid(problems)) = Pack::from_toml(&no_note) else {
        panic!("the broken pack loaded");
    };
    assert!(
        problems
            .iter()
            .any(|p| p.contains("VAT code EUSVC: invoice_note needs both languages")),
        "{problems:?}"
    );
}
