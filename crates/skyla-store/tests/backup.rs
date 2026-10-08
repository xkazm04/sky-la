//! WP-30 acceptance (the store half): scheduled backups with manifests,
//! pruning, and the restore drill, which runs in CI. The drill catches a
//! swapped file, a wrong key, changed content and a chain head that moved.

#![allow(clippy::unwrap_used)]

use rusqlite::params;
use skyla_store::backup::{self, BackupPolicy};
use skyla_store::{DataKey, Migration, Store};

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "notes",
    sql: "CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT NOT NULL) STRICT;",
}];

fn add(store: &Store, body: &str) {
    let body = body.to_owned();
    store
        .write(move |c| {
            c.execute("INSERT INTO note (body) VALUES (?1)", params![body])?;
            Ok(())
        })
        .unwrap();
}

/// The "chain head" of this test schema: the newest note.
fn head(conn: &rusqlite::Connection) -> Option<String> {
    conn.query_row(
        "SELECT max(id) || ':' || body FROM note WHERE id = (SELECT max(id) FROM note)",
        [],
        |r| r.get(0),
    )
    .ok()
}

fn store_head(store: &Store) -> Option<String> {
    store.read(|c| Ok(head(c))).unwrap()
}

#[test]
fn backups_are_scheduled_listed_and_pruned() {
    let dir = tempfile::tempdir().unwrap();
    let key = DataKey::generate().unwrap();
    let store = Store::create(&dir.path().join("books.db"), &key, MIGRATIONS).unwrap();
    let backups = dir.path().join("Backups");
    let policy = BackupPolicy {
        every_days: 1,
        keep: 3,
    };
    assert!(
        backup::is_due(policy, None, "2026-10-01T08:00:00Z"),
        "the first one is always due"
    );
    for (i, day) in [
        "2026-10-01",
        "2026-10-02",
        "2026-10-03",
        "2026-10-04",
        "2026-10-05",
    ]
    .iter()
    .enumerate()
    {
        add(&store, &format!("note {i}"));
        let now = format!("{day}T08:00:00Z");
        let newest = backup::list(&backups, "Jan Novák").unwrap();
        assert!(backup::is_due(policy, newest.first(), &now));
        let b =
            backup::backup_now(&store, &backups, "Jan Novák", &now, store_head(&store)).unwrap();
        assert!(
            b.file
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("skyla-jan-nov-k-")
        );
        assert!(
            !backup::is_due(policy, Some(&b), &format!("{day}T23:00:00Z")),
            "not twice a day"
        );
    }
    let removed = backup::prune(&backups, "Jan Novák", policy.keep).unwrap();
    assert_eq!(removed.len(), 2);
    let left = backup::list(&backups, "Jan Novák").unwrap();
    assert_eq!(left.len(), 3);
    assert_eq!(
        left[0].manifest.created_at, "2026-10-05T08:00:00Z",
        "newest first"
    );
    assert_eq!(left[0].manifest.chain_head.as_deref(), Some("5:note 4"));
    assert_eq!(left[0].manifest.schema_version, 1);
}

#[test]
fn the_restore_drill_passes_for_a_good_backup_and_names_what_is_wrong() {
    let dir = tempfile::tempdir().unwrap();
    let key = DataKey::generate().unwrap();
    let store = Store::create(&dir.path().join("books.db"), &key, MIGRATIONS).unwrap();
    add(&store, "first");
    add(&store, "second");
    let backups = dir.path().join("Backups");
    let scratch = dir.path().join("drill");
    let good = backup::backup_now(
        &store,
        &backups,
        "e",
        "2026-10-07T12:00:00Z",
        store_head(&store),
    )
    .unwrap();
    let report = backup::drill(&good, &key, MIGRATIONS, &scratch, head).unwrap();
    assert!(report.passed(), "{report:?}");
    assert_eq!(report.chain_matches, Some(true));
    assert_eq!(
        std::fs::read_dir(&scratch).unwrap().count(),
        0,
        "the drill cleans up"
    );

    // The wrong key doesn't open it.
    assert!(
        backup::drill(
            &good,
            &DataKey::generate().unwrap(),
            MIGRATIONS,
            &scratch,
            head
        )
        .is_err()
    );

    // A manifest whose chain head isn't the one inside: someone rewrote the books.
    let mut forged = good.clone();
    forged.manifest.chain_head = Some("2:something else".into());
    let r = backup::drill(&forged, &key, MIGRATIONS, &scratch, head).unwrap();
    assert_eq!(r.chain_matches, Some(false));
    assert!(!r.passed());

    // A different file behind the manifest.
    add(&store, "third");
    let later = backup::backup_now(
        &store,
        &backups,
        "e",
        "2026-10-08T12:00:00Z",
        store_head(&store),
    )
    .unwrap();
    std::fs::copy(&later.file, &good.file).unwrap();
    let r = backup::drill(&good, &key, MIGRATIONS, &scratch, head).unwrap();
    assert!(!r.file_matches && !r.passed());
}
