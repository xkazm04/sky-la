//! Improvement wave 2: real books keep their workbench between sessions.
//! The bank imports, bookings and dedup keys, the advisor policies, the
//! reference data and both switches, and a verified rule-pack update all
//! come back when the books are unlocked again. A saved pack update applies
//! only if it still verifies and is still newer.

#![allow(clippy::unwrap_used)]

use std::io::Cursor;

use base64::Engine as _;
use sha2::{Digest, Sha256};
use skyla_app::Core;
use skyla_app::dto::{BankAllocationDto, EntitySetupDto};
use skyla_app::session::Gate;

const PASS: &str = "a long passphrase for the books";

fn setup() -> EntitySetupDto {
    EntitySetupDto {
        display_name: "Eva Malá".into(),
        ico: "27415830".into(),
        dic: Some("CZ8001011234".into()),
        address: "Dlouhá 1, 110 00 Praha 1".into(),
        vat_period: "monthly".into(),
        registration: "Zapsána v živnostenském rejstříku".into(),
        iban: Some("CZ6508000000192000145399".into()),
        bank_name: "Fio".into(),
        email: None,
        flat_rate_group: Some("liberal".into()),
    }
}

/// The Fio sample, opening at zero so it ties out against new books.
fn statement() -> String {
    include_str!("../../skyla-bank/tests/samples/fio.csv")
        .replace(
            "\"openingBalance\";\"10000,00\"",
            "\"openingBalance\";\"0,00\"",
        )
        .replace(
            "\"closingBalance\";\"7450,00\"",
            "\"closingBalance\";\"-2550,00\"",
        )
}

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

#[test]
fn the_workbench_comes_back_after_unlocking_again() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::reproducible(dir.path().to_path_buf());
    let (core, _) = gate.create(&setup(), PASS).unwrap();
    let s = core
        .import_bank_statement("fio.csv", &b64(&statement()))
        .unwrap();
    let fee = s
        .lines
        .iter()
        .find(|l| l.amount.minor == -57_500)
        .unwrap()
        .id
        .clone();
    core.book_bank_line(
        &fee,
        &[BankAllocationDto {
            entry_id: None,
            account: Some("568".into()),
            vat_code: None,
            amount: "575,00".into(),
        }],
    )
    .unwrap();
    core.set_egress_policy("tax.scenarios", "never").unwrap();
    core.set_reference_fetch(true).unwrap();
    core.import_reference_data(
        "cnb_repo",
        "repo.csv",
        include_str!("../../../packages/fixtures/data/refdata/demo-cnb-repo-history.csv"),
    )
    .unwrap();
    core.set_update_check(true);
    drop(core);

    let core = gate.unlock(PASS, false).unwrap();
    let s = core.bank_statement().unwrap();
    assert_eq!(s.lines.len(), 3);
    let booked = s.lines.iter().find(|l| l.id == fee).unwrap();
    assert_eq!(booked.status, "booked");
    assert!(booked.entry_id.is_some());
    let again = core.import_bank_statement("fio.csv", &b64(&statement()));
    assert!(again.unwrap_err().to_string().contains("imported before"));
    assert_eq!(
        core.egress_policy("tax.scenarios"),
        skyla_egress::Policy::Never
    );
    let r = core.reference_data().unwrap();
    assert!(r.fetch_enabled);
    assert!(r.sources.iter().any(|x| x.origin == "imported repo.csv"));
    assert!(core.update_status().enabled);
}

fn bumped() -> String {
    skyla_rules::CZ_2026.replacen("version = \"2026.1\"", "version = \"2026.2\"", 1)
}

#[test]
fn a_verified_pack_update_applies_from_the_next_opening() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::reproducible(dir.path().to_path_buf());
    let (core, _) = gate.create(&setup(), PASS).unwrap();
    let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let key = kp.pk.to_base64();
    let toml = bumped();
    let sig = minisign::sign(
        Some(&kp.pk),
        &kp.sk,
        Cursor::new(toml.as_bytes()),
        None,
        None,
    )
    .unwrap()
    .to_string();
    let r = core
        .install_pack_update_trusting(&toml, &sig, &[&key])
        .unwrap();
    assert_eq!(r.in_use, "cz-2026@2026.1");
    drop(core);

    let data_key =
        skyla_store::DataKey::from_bytes(Sha256::digest(b"sky-la reproducible key 0").into());
    let books = dir.path().join("books.db");
    let core = Core::open_entity_trusting(&books, &data_key, "2026-10-07", &[&key]).unwrap();
    assert_eq!(core.rule_pack().provenance, "cz-2026@2026.2");
    drop(core);
    // Not trusted any more (or never): the built-in pack.
    let core = Core::open_entity_trusting(&books, &data_key, "2026-10-07", &[]).unwrap();
    assert_eq!(core.rule_pack().provenance, "cz-2026@2026.1");
}

#[test]
fn the_inbox_keeps_advisor_items_and_dismissals_after_unlocking_again() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::reproducible(dir.path().to_path_buf());
    let (core, _) = gate.create(&setup(), PASS).unwrap();
    let s = core
        .import_bank_statement("fio.csv", &b64(&statement()))
        .unwrap();
    let line = s
        .lines
        .iter()
        .find(|l| l.status != "booked" && l.amount.minor < 0)
        .unwrap()
        .id
        .clone();
    let cat = core
        .call_tool(
            "propose_categorisation",
            &serde_json::json!({ "line": line, "account": "518", "reason": "A service." }),
        )
        .unwrap();
    let asked = core
        .call_tool(
            "ask_user",
            &serde_json::json!({ "question": "Do you claim a child?", "why": "The bonus." }),
        )
        .unwrap();
    core.dismiss_proposal(asked["proposal"].as_str().unwrap())
        .unwrap();
    drop(core);

    let core = gate.unlock(PASS, false).unwrap();
    let ids: Vec<String> = core
        .proposals()
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();
    let cat = cat["proposal"].as_str().unwrap().to_owned();
    assert!(ids.contains(&cat), "{ids:?}");
    assert!(
        !ids.iter()
            .any(|id| id == asked["proposal"].as_str().unwrap())
    );
    core.approve_proposals(std::slice::from_ref(&cat)).unwrap();
    drop(core);

    let core = gate.unlock(PASS, false).unwrap();
    let booked = core
        .bank_statement()
        .unwrap()
        .lines
        .into_iter()
        .find(|l| l.id == line)
        .unwrap();
    assert_eq!(booked.status, "booked");
    assert!(core.proposals().unwrap().iter().all(|p| p.id != cat));
    let next = core
        .call_tool(
            "ask_user",
            &serde_json::json!({ "question": "Anything else?", "why": "To finish." }),
        )
        .unwrap();
    assert_eq!(next["proposal"], "advisor-3", "ids aren't reused");
    assert!(core.integrity().unwrap().chain_intact);
}
