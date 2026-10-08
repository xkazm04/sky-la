//! SQLCipher registers `sqlcipher_export` at the very end of its library
//! initialisation, so threads that open their first connections at the same
//! moment could get one without it, and a backup failed with "no such
//! function: sqlcipher_export". This binary is its own process, so its
//! threads really are the first to open connections.

#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Barrier};

use skyla_store::{DataKey, export_encrypted, open_keyed};

#[test]
fn threads_opening_their_first_connections_together_can_all_export() {
    let dir = tempfile::tempdir().unwrap();
    let threads = 16;
    let start = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|i| {
            let start = start.clone();
            let dir = dir.path().to_path_buf();
            std::thread::spawn(move || {
                let key = DataKey::from_bytes([u8::try_from(i).unwrap(); 32]);
                start.wait();
                let conn = open_keyed(&dir.join(format!("books-{i}.db")), &key, true).unwrap();
                conn.execute_batch("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);")
                    .unwrap();
                export_encrypted(&conn, &key, &dir.join(format!("copy-{i}.db"))).unwrap();
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}
