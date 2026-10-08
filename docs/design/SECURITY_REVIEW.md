# Security review (WP-32)

October 2026, at the end of M7's feature work (WP-00 – WP-31). The review checks the security model and threat table in [`DESIGN.md` §3.8](DESIGN.md) against the code, fixes what it finds, and turns the checks into gates that run on every push, so later changes can't quietly undo them.

## What runs on every push

| Gate | Where | What it holds |
|---|---|---|
| Threat-model tests | `crates/skyla-app/tests/threats.rs` (in `just test`) | One test per threat below, against the real encrypted store and the core the shell uses |
| Security audit | `scripts/check_security.py` (`just security-check`, part of `just ci`) | CSP, capabilities, no Tauri plugins, network paths, process spawns, no network calls in the webview |
| Navigation guard | `navigation_allowed` in the shell, tested in `apps/desktop/src-tauri/tests/ipc.rs` (`just check-desktop`) | The app window shows only the app's own pages |
| Licence boundary | `scripts/check_licence_boundary.py` | Apache-2.0 engine crates never depend on AGPL crates |
| `cargo-deny` | `deny.toml` (CI job) | Advisories (each exception names its reason), licences, crates.io only, no wildcards |
| `pnpm audit` | CI job `npm-audit` | No known moderate-or-worse advisories in the web dependencies |
| Pinned CI | `.github/workflows/ci.yml` | Every action pinned to a commit; the workflow token can only read |

## Threats and the tests that hold them

| Threat (DESIGN §3.8) | Mitigation | Tests |
|---|---|---|
| Stolen laptop | SQLCipher books and backups under a data key the passphrase or the recovery key unwraps | `stolen_laptop_finds_only_ciphertext`: the books, the vault and a backup contain no name, memo, IBAN, passphrase or SQLite header; a wrong passphrase opens nothing. Store: `crates/skyla-store/tests` |
| Malicious bank or import file | Parsers in Rust, outside the webview; DTDs refused; fuzzed bank parsers | `malicious_files_are_refused_without_effect`: an entity bomb is refused by the bank and invoice importers, 800 mangled samples give problems and never a panic, and nothing is written. Fuzzing: `just fuzz` (CI, non-blocking) with the corpus replayed in `skyla-bank` tests |
| Prompt injection via memos | Tools only read, compute or propose; the kernel checks proposals; a person approves | `an_injected_model_can_only_propose`: no tool name is a write, unknown tools are refused, an injected proposal lands in the inbox with the journal unchanged, an unbalanced one is refused |
| Exfiltration via the model | The gate covers prompts and tool results; pseudonyms; scopes; the register | `tool_results_leave_without_identifiers`: the raw result has an account number and a supplier's name, and the gated one has neither, under every task. Property tests: `crates/skyla-egress` |
| Compromised webview | It holds no key; the core re-validates every command; no fs, shell or HTTP | `a_compromised_webview_gets_no_key_and_no_shortcut`: no recorded answer carries the data key or the passphrase; malformed drafts, dates and bank lines are refused with the journal unchanged. The audit holds the capabilities and CSP |
| Someone at an unlocked computer | The full export (unencrypted by nature) asks for the passphrase again | `exporting_real_books_needs_the_passphrase_again` |
| Silent corruption | SQL triggers freeze posted entries; the hash chain names the first break | `tampering_with_the_real_file_is_refused_or_caught`: edits and deletes on the real file are refused; with the triggers dropped, the chain catches the edit on the next open. Ledger: `crates/skyla-ledger/tests/close.rs` |
| Lost passphrase | The recovery key, which is then replaced | `a_lost_passphrase_is_recovered_once_per_key` |

## Findings

### Fixed in WP-32

1. **The export didn't ask again** (medium). DESIGN §3.8 calls for re-authentication before exports; the zip leaves the encryption behind. For real books, `export_books` now asks the gate to confirm the passphrase (`Gate::confirm_passphrase`), and Settings asks for it.
2. **The app window could be navigated to a remote page** (medium). Remote pages get no IPC under Tauri's capabilities, but one could still sit in the app's window and look like the unlock screen. A navigation guard now allows only the app's own origin (and the dev server in debug builds).
3. **The restore drill used a shared, predictable temp folder** (low). Another account on the machine could pre-create or swap `/tmp/skyla-drill` and make the drill report a pass. It now restores into `.restore-drill` beside the books, in the user's own folder, and cleans up.
4. **CI actions weren't pinned and the token had default permissions** (medium, supply chain). Every action is now pinned to a commit with its tag in a comment, and `permissions: contents: read` applies to the whole workflow.
5. **No gate on web-dependency advisories** (low). The `npm-audit` job fails on moderate or worse.

### Checked, no change needed

- **CSP:** `default-src`, `script-src`, `font-src` `'self'`; `object-src`, `base-uri`, `frame-ancestors`, `form-action` `'none'`; `connect-src` only Tauri IPC; images `'self'` and `data:`; `freezePrototype` on; no global Tauri object; asset protocol off.
- **Capabilities:** the main window gets `core:default` only, with no remote origins; the shell links no Tauri plugin and the webview depends on none.
- **Network:** the only HTTP client is `ureq`, linked by `skyla-app`. Two modules call it, both off by default and each with one fixed host: reference data (the ČNB) and, since WP-33, the update check (the project's GitHub releases, and only once the build trusts a release key). The only sockets are the advisor tool host's loopback listener (a fresh 244-bit token per run, compared in constant time) and the MCP shim's loopback client. Tauri links `reqwest` on Android and iOS only (its mobile dev-server proxy), which sky-la doesn't build; the audit reads the dependency graph for the three desktop targets.
- **Processes:** the only spawn is the user's own `claude` binary, with the hardened profile, an allow-listed environment and stdin closed (WP-24).
- **Secrets:** the data key never crosses IPC; the passphrase only goes in; the recovery key comes out once, by design, to be written down. Key material is zeroised in `skyla-store`.
- **Parsers:** both XML readers (camt.053 and the Pohoda importer) use `roxmltree` with `allow_dtd: false` and a node limit, so entity expansion never happens and a huge file stops early.
- **Install scripts:** pnpm 10 runs no dependency lifecycle scripts unless allowed, and none are allowed.

### Accepted

- **`style-src 'unsafe-inline'`:** React Aria positions overlays with inline styles. Scripts stay `'self'` only, so this allows styling, not code.
- **The unlock screen's label** (`entity.json`) holds the display name in the clear, so the screen can say whose books these are. Nothing else is outside the encrypted file.
- **The demo** keeps its invented books in memory, unencrypted; it never holds user data.
- **Over-redaction:** the gate treats any counterparty as a party, so "Poplatek za vedení účtu" (a bank fee) leaves as "Vendor H". That is safe, if odd.

### Open (in `STATUS.md` → Backlog notes)

- **Auto-lock on idle and lock on sleep** aren't built. Locking means dropping the open books, which needs the shell's commands to reach the core through the session rather than Tauri's managed state. That refactor touches every command, so it gets its own change.
- **Tauri's isolation pattern** isn't enabled. It needs a run on a real webview to verify, so it goes with packaging (WP-33).
- **Statute links** in Settings no longer replace the app window. Opening them in the system browser, for rule-pack citation URLs only, needs an opener, and adding one is a decision for the maintainer.
- **SBOM, signed and notarised releases, reproducible builds:** WP-33.
- **Keeping the pinned actions current** needs Dependabot or Renovate, also a maintainer decision.
