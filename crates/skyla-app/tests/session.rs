//! WP-30 acceptance (the app half): a new entity's books are created
//! encrypted, the recovery key is shown once and must be confirmed, the
//! passphrase, the keychain and the recovery key each unlock them, and a
//! backup of real books passes the restore drill with its chain head.

#![allow(clippy::unwrap_used)]

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

const PASS: &str = "a long passphrase for the books";

/// The refusal's message (the Ok side holds a `Core`, which isn't `Debug`).
fn refusal<T>(r: Result<T, skyla_app::CoreError>) -> String {
    match r {
        Err(e) => e.to_string(),
        Ok(_) => panic!("expected a refusal"),
    }
}

#[test]
fn a_new_entity_is_created_encrypted_and_its_recovery_key_must_be_confirmed() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::reproducible(dir.path().to_path_buf());
    assert_eq!(gate.state().state, "needs_setup");
    let (core, shown) = gate.create(&setup(), PASS).unwrap();
    assert_eq!(core.entity().display_name, "Eva Malá");
    assert_eq!(shown.key.split('-').count() as u32, shown.groups);
    // The books are SQLCipher, not plain SQLite.
    let head = std::fs::read(dir.path().join("books.db")).unwrap();
    assert!(!head.starts_with(b"SQLite format 3\0"));
    assert!(!gate.confirm_recovery_key("WRONG").unwrap());
    let last = shown.key.rsplit('-').next().unwrap().to_lowercase();
    assert!(
        gate.confirm_recovery_key(&last).unwrap(),
        "case doesn't matter"
    );
    assert!(
        gate.confirm_recovery_key(&last).is_err(),
        "nothing left to confirm"
    );
    // A second setup is refused.
    assert!(gate.create(&setup(), PASS).is_err());
    let state = gate.state();
    assert_eq!(
        (state.state.as_str(), state.entity.as_deref()),
        ("locked", Some("Eva Malá"))
    );
}

#[test]
fn the_setup_form_is_checked_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::reproducible(dir.path().to_path_buf());
    let mut bad = setup();
    bad.ico = "12345678".into();
    bad.dic = None;
    let err = refusal(gate.create(&bad, PASS));
    assert!(err.contains("IČO") && err.contains("DIČ"), "{err}");
    assert!(
        gate.create(&setup(), "short").is_err(),
        "a short passphrase is refused"
    );
}

#[test]
fn passphrase_keychain_and_recovery_key_each_unlock_the_books() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::reproducible(dir.path().to_path_buf());
    let (core, shown) = gate.create(&setup(), PASS).unwrap();
    drop(core);
    assert!(refusal(gate.unlock("not the passphrase", false)).contains("doesn't open"));
    let core = gate.unlock(PASS, true).unwrap();
    assert_eq!(core.entity().display_name, "Eva Malá");
    drop(core);
    assert!(gate.state().remembered);
    assert!(gate.unlock_remembered().unwrap().is_some());
    // Forgot the passphrase: the recovery key opens the books and is replaced.
    let (core, fresh) = gate
        .recover(&shown.key, "a brand new passphrase here")
        .unwrap();
    drop(core);
    assert_ne!(fresh.key, shown.key);
    assert!(
        gate.recover(&shown.key, "another new passphrase").is_err(),
        "the used key is spent"
    );
    assert!(
        gate.unlock(PASS, false).is_err(),
        "the old passphrase is gone"
    );
    assert!(gate.unlock("a brand new passphrase here", false).is_ok());
    assert!(!gate.state().remembered, "recovery forgets the cached key");
}

#[test]
fn real_books_back_up_and_pass_the_restore_drill() {
    let dir = tempfile::tempdir().unwrap();
    let gate = Gate::reproducible(dir.path().to_path_buf());
    let (core, _) = gate.create(&setup(), PASS).unwrap();
    assert_eq!(core.backups().unwrap().backups.len(), 0);
    let first = core.backup_now("2026-10-07T12:00:00Z").unwrap();
    assert!(first.chain_head.is_none(), "no entries yet");
    assert!(
        core.backup_if_due("2026-10-07T18:00:00Z")
            .unwrap()
            .is_none(),
        "one a day"
    );
    assert!(
        core.backup_if_due("2026-10-08T09:00:00Z")
            .unwrap()
            .is_some()
    );
    let drill = core.restore_drill().unwrap();
    assert!(drill.passed, "{drill:?}");
    assert_eq!(core.backups().unwrap().backups.len(), 2);
}
