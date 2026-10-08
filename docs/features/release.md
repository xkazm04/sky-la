# Release and updates

How sky-la is packaged and signed, and how it can (only if asked) learn that a new version exists.

**Code:** `crates/skyla-release`, `crates/skyla-app/src/core/update.rs`, `.github/workflows/release.yml`, `apps/desktop/src-tauri/tauri.release.conf.json` · **Docs:** `docs/RELEASING.md`, `CHANGELOG.md`

## Packaging

- A `v*` tag runs `release.yml`, which builds:
  - .deb and AppImage (on Ubuntu 22.04);
  - .dmg for both kinds of Mac;
  - an NSIS installer for Windows.
- Actions are pinned to commits, with a read-only token; only the publish job may write.
- macOS is signed and notarised, and Windows Authenticode-signed, when the maintainer's secrets are set.
- The `skyla-mcp` shim ships as a sidecar beside the `sky-la` binary.
- `skyla-release` checks that the tag, the crates, `tauri.conf.json` and `CHANGELOG.md` agree on the version.
- It collects the bundles with `SHA256SUMS`, a CycloneDX SBOM and the update manifest (`latest.json`), signs them with the maintainer's minisign key and drafts the release.
- `just release-dry-run` builds and checks the .deb locally.

## The opt-in update check

- It's off by default.
- When it's on, the check believes only a manifest signed by a trusted release key that points at the project's own releases.
- It never downloads anything; it tells the user that a new version exists.
- `TRUSTED_RELEASE_KEYS` is empty until the maintainer creates the key, so for now the check fetches nothing.

## Not yet

- The release workflow hasn't run on a real tag. Check its first run, especially the AppImage step and the SBOM flags.
- The maintainer's signing keys (release and rule pack) and the CI secrets need creating (`docs/RELEASING.md`).
- Bundles aren't bit-for-bit reproducible (signing, timestamps, toolchain paths).
