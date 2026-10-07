//! WP-03 acceptance tests for encrypted storage and key management.

use std::fs;

use rusqlite::params;
use skyla_store::{DataKey, KdfParams, Migration, RecoveryKey, Store, StoreError, Vault};

/// Cheap Argon2 parameters so tests run fast. Production uses `KdfParams::RECOMMENDED`.
const TEST_KDF: KdfParams = KdfParams {
    m_cost_kib: 64,
    t_cost: 1,
    p_cost: 1,
};
const PASSPHRASE: &str = "correct horse battery staple";

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "notes",
        sql: "CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT NOT NULL) STRICT;",
    },
    Migration {
        version: 2,
        name: "note_amount",
        sql: "ALTER TABLE note ADD COLUMN amount_minor INTEGER NOT NULL DEFAULT 0;",
    },
];

fn add_note(store: &Store, body: &str, amount: i64) {
    let body = body.to_owned();
    store
        .write(move |conn| {
            conn.execute(
                "INSERT INTO note (body, amount_minor) VALUES (?1, ?2)",
                params![body, amount],
            )?;
            Ok(())
        })
        .expect("insert note");
}

fn count_notes(store: &Store) -> i64 {
    store
        .read(|conn| Ok(conn.query_row("SELECT count(*) FROM note", [], |r| r.get(0))?))
        .expect("count notes")
}

#[test]
fn the_file_is_unreadable_without_the_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("books.db");
    let key = DataKey::generate().unwrap();
    {
        let store = Store::create(&path, &key, MIGRATIONS).unwrap();
        add_note(&store, "Northwind paid 84 700,00", 8_470_000);
    }

    let bytes = fs::read(&path).unwrap();
    assert!(
        !bytes.starts_with(b"SQLite format 3\0"),
        "plain SQLite header means the file is not encrypted"
    );
    assert!(
        !bytes.windows(9).any(|w| w == b"Northwind"),
        "plaintext leaked into the file"
    );

    // A plain connection with no key can't read it.
    let plain = rusqlite::Connection::open(&path).unwrap();
    assert!(
        plain
            .query_row("SELECT count(*) FROM sqlite_master", [], |r| r
                .get::<_, i64>(0))
            .is_err()
    );

    // The wrong key is reported as such, not as corruption.
    let wrong = DataKey::generate().unwrap();
    assert!(matches!(
        Store::open(&path, &wrong, MIGRATIONS),
        Err(StoreError::WrongSecret)
    ));

    // The right key opens it with the data intact.
    let store = Store::open(&path, &key, MIGRATIONS).unwrap();
    assert_eq!(count_notes(&store), 1);
}

#[test]
fn migrations_apply_once_in_order_and_newer_schemas_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("books.db");
    let key = DataKey::generate().unwrap();

    let store = Store::create(&path, &key, &MIGRATIONS[..1]).unwrap();
    assert_eq!(store.schema_version().unwrap(), 1);
    drop(store);

    let store = Store::open(&path, &key, MIGRATIONS).unwrap();
    assert_eq!(store.schema_version().unwrap(), 2);
    add_note(&store, "after v2", 1);
    drop(store);

    // Reopening applies nothing new.
    assert_eq!(
        Store::open(&path, &key, MIGRATIONS)
            .unwrap()
            .schema_version()
            .unwrap(),
        2
    );
    // An older build must not open a newer database.
    assert!(matches!(
        Store::open(&path, &key, &MIGRATIONS[..1]),
        Err(StoreError::NewerSchema {
            found: 2,
            supported: 1
        })
    ));

    let gap = [Migration {
        version: 2,
        name: "x",
        sql: "",
    }];
    let other = dir.path().join("other.db");
    assert!(matches!(
        Store::create(&other, &key, &gap),
        Err(StoreError::InvalidMigrations(_))
    ));
}

#[test]
fn a_failed_write_rolls_back_when_it_uses_a_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::create(
        &dir.path().join("books.db"),
        &DataKey::generate().unwrap(),
        MIGRATIONS,
    )
    .unwrap();
    let result = store.write(|conn| {
        let tx = conn.transaction()?;
        tx.execute("INSERT INTO note (body) VALUES ('first')", [])?;
        tx.execute("INSERT INTO note (body) VALUES (NULL)", [])?; // violates NOT NULL
        tx.commit()?;
        Ok(())
    });
    assert!(matches!(result, Err(StoreError::Sql(_))));
    assert_eq!(count_notes(&store), 0);
}

#[test]
fn writes_from_many_threads_are_serialised() {
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(
        Store::create(
            &dir.path().join("books.db"),
            &DataKey::generate().unwrap(),
            MIGRATIONS,
        )
        .unwrap(),
    );
    let handles: Vec<_> = (0..8)
        .map(|t| {
            let store = store.clone();
            std::thread::spawn(move || {
                for i in 0..25 {
                    add_note(&store, &format!("{t}-{i}"), i);
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(count_notes(&store), 200);
}

#[test]
fn rekey_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("books.db");
    let old = DataKey::generate().unwrap();
    let new = DataKey::generate().unwrap();
    {
        let mut store = Store::create(&path, &old, MIGRATIONS).unwrap();
        add_note(&store, "before rekey", 5);
        let before = store.content_hash().unwrap();
        store.rekey(&new).unwrap();
        assert_eq!(
            store.content_hash().unwrap(),
            before,
            "reader reopened under the new key"
        );
        add_note(&store, "after rekey", 6);
    }
    assert!(matches!(
        Store::open(&path, &old, MIGRATIONS),
        Err(StoreError::WrongSecret)
    ));
    assert_eq!(
        count_notes(&Store::open(&path, &new, MIGRATIONS).unwrap()),
        2
    );
}

#[test]
fn the_recovery_key_opens_the_books_after_the_passphrase_is_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("books.db");
    let vault_path = dir.path().join("books.vault.json");

    let shown_recovery = {
        let (vault, key, recovery) = Vault::create(PASSPHRASE, TEST_KDF).unwrap();
        vault.save(&vault_path).unwrap();
        let store = Store::create(&db, &key, MIGRATIONS).unwrap();
        add_note(&store, "kept for ten years", 1);
        recovery.to_display().to_string()
    };

    // Passphrase forgotten: the user types the printed recovery key.
    let mut vault = Vault::load(&vault_path).unwrap();
    assert!(matches!(
        vault.unlock_with_passphrase("not the passphrase!"),
        Err(StoreError::WrongSecret)
    ));
    let key = vault
        .unlock_with_recovery(&RecoveryKey::parse(&shown_recovery).unwrap())
        .unwrap();
    assert_eq!(count_notes(&Store::open(&db, &key, MIGRATIONS).unwrap()), 1);

    // They set a new passphrase; the database itself is untouched.
    vault
        .change_passphrase(&key, "a brand new passphrase")
        .unwrap();
    vault.save(&vault_path).unwrap();
    let vault = Vault::load(&vault_path).unwrap();
    assert!(matches!(
        vault.unlock_with_passphrase(PASSPHRASE),
        Err(StoreError::WrongSecret)
    ));
    assert_eq!(
        vault
            .unlock_with_passphrase("a brand new passphrase")
            .unwrap(),
        key
    );
    assert_eq!(
        vault
            .unlock_with_recovery(&RecoveryKey::parse(&shown_recovery).unwrap())
            .unwrap(),
        key
    );
}

#[test]
fn the_vault_resists_tampering_and_misuse() {
    let (mut vault, key, recovery) = Vault::create(PASSPHRASE, TEST_KDF).unwrap();
    assert!(matches!(
        Vault::create("short", TEST_KDF),
        Err(StoreError::PassphraseTooShort { .. })
    ));
    assert!(matches!(
        vault.change_passphrase(&key, "short"),
        Err(StoreError::PassphraseTooShort { .. })
    ));

    // A different key can't re-wrap the vault.
    let stranger = DataKey::generate().unwrap();
    assert!(matches!(
        vault.change_passphrase(&stranger, "a perfectly long passphrase"),
        Err(StoreError::WrongSecret)
    ));
    assert!(matches!(
        vault.rotate_recovery_key(&stranger),
        Err(StoreError::WrongSecret)
    ));

    // Flipping one ciphertext byte is detected.
    let json = vault.to_json().unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ct = value["by_passphrase"]["ciphertext"]
        .as_str()
        .unwrap()
        .to_owned();
    let flipped = format!(
        "{}{}",
        if ct.starts_with('0') { "1" } else { "0" },
        &ct[1..]
    );
    value["by_passphrase"]["ciphertext"] = serde_json::Value::String(flipped);
    let tampered = Vault::from_json(&value.to_string()).unwrap();
    assert!(matches!(
        tampered.unlock_with_passphrase(PASSPHRASE),
        Err(StoreError::WrongSecret)
    ));

    // Swapping the two wrapped copies is detected (they're bound to their purpose).
    let mut swapped: serde_json::Value = serde_json::from_str(&json).unwrap();
    let pass = swapped["by_passphrase"].clone();
    swapped["by_passphrase"] = swapped["by_recovery"].clone();
    swapped["by_recovery"] = pass;
    let swapped = Vault::from_json(&swapped.to_string()).unwrap();
    assert!(matches!(
        swapped.unlock_with_recovery(&recovery),
        Err(StoreError::WrongSecret)
    ));

    // Unknown versions are refused; garbage is a format error.
    let mut future: serde_json::Value = serde_json::from_str(&json).unwrap();
    future["version"] = 99.into();
    assert!(matches!(
        Vault::from_json(&future.to_string()),
        Err(StoreError::VaultVersion(99))
    ));
    assert!(matches!(
        Vault::from_json("{}"),
        Err(StoreError::VaultFormat(_))
    ));

    // Rotating the recovery key retires the old one.
    let fresh = vault.rotate_recovery_key(&key).unwrap();
    assert!(matches!(
        vault.unlock_with_recovery(&recovery),
        Err(StoreError::WrongSecret)
    ));
    assert_eq!(vault.unlock_with_recovery(&fresh).unwrap(), key);
}

#[test]
fn backup_then_restore_preserves_content_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let key = DataKey::generate().unwrap();
    let store = Store::create(&dir.path().join("books.db"), &key, MIGRATIONS).unwrap();
    for i in 0..50 {
        add_note(&store, &format!("entry {i}"), i * 100);
    }
    let original = store.content_hash().unwrap();

    let backup = dir.path().join("backup.db");
    store.backup_to(&backup).unwrap();
    assert!(matches!(
        store.backup_to(&backup),
        Err(StoreError::AlreadyExists(_))
    ));
    assert!(
        !fs::read(&backup).unwrap().starts_with(b"SQLite format 3\0"),
        "the backup is encrypted too"
    );

    // Only the same key opens the backup.
    let elsewhere = dir.path().join("restored.db");
    assert!(matches!(
        Store::restore(&backup, &elsewhere, &DataKey::generate().unwrap()),
        Err(StoreError::WrongSecret)
    ));
    assert!(!elsewhere.exists());

    Store::restore(&backup, &elsewhere, &key).unwrap();
    let restored = Store::open(&elsewhere, &key, MIGRATIONS).unwrap();
    assert_eq!(restored.content_hash().unwrap(), original);
    assert_eq!(count_notes(&restored), 50);

    // Content hash notices a single changed value.
    add_note(&restored, "one more", 1);
    assert_ne!(restored.content_hash().unwrap(), original);
}

#[test]
fn create_and_open_refuse_the_wrong_situation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("books.db");
    let key = DataKey::generate().unwrap();
    assert!(matches!(
        Store::open(&path, &key, MIGRATIONS),
        Err(StoreError::NotFound(_))
    ));
    drop(Store::create(&path, &key, MIGRATIONS).unwrap());
    assert!(matches!(
        Store::create(&path, &key, MIGRATIONS),
        Err(StoreError::AlreadyExists(_))
    ));
}
