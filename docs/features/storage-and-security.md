# Encrypted storage and sessions

The books are one SQLCipher file per entity. The keys never reach the webview.

**Code:** `crates/skyla-store`, `crates/skyla-app/src/session.rs`, `crates/skyla-app/src/core/{entity,persist}.rs`, `apps/desktop/src-tauri/src/lib.rs` (`Session`, `Books`) · **Review:** `docs/design/SECURITY_REVIEW.md`

## Keys

- The database is encrypted with a random 256-bit data key. The key doesn't change when the passphrase does.
- A `Vault` wraps the key twice with XChaCha20-Poly1305: once under Argon2id(passphrase), and once under a printable **recovery key**. Each wrap has purpose-bound AAD, a key-check value and an atomic save.
- Keys are compared in constant time and zeroised on drop.
- `OsKeyStore` (keyring 4.2) can remember the data key in the OS keychain. `MemoryKeyStore` covers tests and headless sessions.

## The store

- `Store` has one writer thread (every write goes through it, in order) and a reader connection.
- Migrations have gapless versions, and a database with a newer schema is refused.
- `skyla-store` waits once per process until SQLCipher has finished initialising. This fixed a real race in which an early connection lacked `sqlcipher_export`.

## Sessions: setup, unlock, recover, lock

- `session::Gate` handles the books' lifecycle:
  - **Setup** creates the books and shows the recovery key once; the user confirms it by its last group.
  - **Unlock** takes the passphrase (optionally remembering the key in the keychain) or a remembered key.
  - **Recovery** takes the recovery key, sets a new passphrase and replaces the recovery key.
- The shell's `Session` holds the open books as `RwLock<Option<Arc<Core>>>`. Every command takes a `Books` argument that fetches them, or refuses with `locked`.
- **Auto-lock:** real books lock after 15 minutes without input (`apps/desktop/src/idleLock.ts`), or from Settings → Encryption → **Lock now**. Locking drops the core, its key and its connection. The demo doesn't lock.

## What real books keep besides the journal

`core::persist` writes JSON documents to an `app_state` table inside the same encrypted file after every change:

| Key | Contents |
|---|---|
| `bank` | Imported statements, rules, bookings, dedupe keys, learnt payer accounts, undo counts |
| `egress_policies` | What each advisor task may share |
| `reference_data` | ČNB rates and repo history, and the fetch switch |
| `update_check` | The update-check switch |
| `pack_update` | A verified rule-pack update, re-verified at every opening |
| `inbox` | What advisors filed, and dismissed advice |

Received invoices have their own `received_invoice` table. The business profile is in `app_profile`. Settings → Business details changes it, and the shell reopens the books so the change applies at once.

## Backups and the restore drill

- A backup is an SQLCipher export under the same data key, so the passphrase and the recovery key both open it.
- Each backup has a JSON manifest: time, schema version, content hash, the file's SHA-256 and size, and the journal's chain head.
- Backups run when one is due and are pruned to the newest N.
- The **restore drill** restores a backup into a private scratch folder and checks the file, the content and the chain head against the manifest. CI runs it.

## Security review (WP-32)

- `crates/skyla-app/tests/threats.rs` has one test per threat in DESIGN §3.8, run on a real encrypted entity.
- `scripts/check_security.py` (`just security-check`, part of `just ci`) audits:
  - the CSP and Tauri capabilities, and that no plugins are used;
  - that `ureq` stays the only HTTP client;
  - that every socket, HTTP call and process spawn sits in its one allowed file.
- The app window has a navigation guard, and the full export asks for the passphrase again.
- CI actions are pinned to commits with a read-only token, and a dependency-audit job runs.

## Not yet

- Lock on sleep (Tauri has no event for it yet).
- Tauri's isolation pattern (needs verifying on a real webview with packaging).
- A user-chosen backup folder; backups sit beside the books.
