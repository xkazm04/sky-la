//! Improvement wave 7: the business details set up on first run can be
//! corrected later. Changes are checked like the setup form and kept in the
//! books; the VAT status isn't changed here, and the demo's are fixed.

#![allow(clippy::unwrap_used)]

use skyla_app::Core;
use skyla_app::dto::EntitySetupDto;
use skyla_app::session::Gate;

fn setup() -> EntitySetupDto {
    EntitySetupDto {
        display_name: "Eva Malá".into(),
        ico: "27415830".into(),
        dic: Some("CZ8001011234".into()),
        address: "Dlouhá 1, 110 00 Praha 1".into(),
        vat_period: "monthly".into(),
        registration: "Zapsána v živnostenském rejstříku".into(),
        iban: Some("CZ6508000000192000145399".into()),
        bank_name: "ČSOB".into(),
        email: None,
        flat_rate_group: Some("liberal".into()),
    }
}

#[test]
fn details_are_corrected_and_survive_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::reproducible(dir.path().to_path_buf());
    let (core, _) = gate
        .create(&setup(), "a long passphrase for the books")
        .unwrap();
    let mut changed = core.profile();
    assert_eq!(changed, setup());
    changed.address = "Krátká 2, 602 00 Brno".into();
    changed.email = Some("eva@example.cz".into());
    core.update_profile(&changed).unwrap();
    let fresh = core.reopen().unwrap();
    assert_eq!(fresh.profile().address, "Krátká 2, 602 00 Brno");
    assert_eq!(fresh.profile().email.as_deref(), Some("eva@example.cz"));
    drop((core, fresh));
    let again = gate
        .unlock("a long passphrase for the books", false)
        .unwrap();
    assert_eq!(again.profile().address, "Krátká 2, 602 00 Brno");
}

#[test]
fn bad_details_and_a_vat_change_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::reproducible(dir.path().to_path_buf());
    let (core, _) = gate
        .create(&setup(), "a long passphrase for the books")
        .unwrap();
    let mut bad = core.profile();
    bad.ico = "12345678".into();
    bad.address = " ".into();
    let refused = core.update_profile(&bad).unwrap_err().to_string();
    assert!(
        refused.contains("IČO") && refused.contains("address"),
        "{refused}"
    );
    let mut iban = core.profile();
    iban.iban = Some("CZ0008000000192000145399".into());
    assert!(
        core.update_profile(&iban).is_err(),
        "a wrong IBAN check digit"
    );
    let mut vat = core.profile();
    vat.vat_period = "quarterly".into();
    assert!(
        core.update_profile(&vat)
            .unwrap_err()
            .to_string()
            .contains("VAT status")
    );
    assert_eq!(core.reopen().unwrap().profile(), setup(), "nothing changed");
}

#[test]
fn the_demo_keeps_its_details() {
    let core = Core::demo().unwrap();
    let p = core.profile();
    assert_eq!(p.display_name, "Jan Novák");
    assert!(core.update_profile(&p).is_err());
}
