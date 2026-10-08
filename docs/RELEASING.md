# Releasing sky-la

A release is a tag. `.github/workflows/release.yml` builds the bundles on each platform, signs what it has keys for, and opens a **draft** GitHub release for the maintainer to read and publish.

## What a release contains

| File | From |
|---|---|
| `sky-la_<version>_amd64.deb`, `sky-la_<version>_amd64.AppImage` | Linux, built on Ubuntu 22.04 so it runs on older glibc |
| `sky-la_<version>_aarch64.dmg`, `sky-la_<version>_x64.dmg` | macOS 11 or later, Apple silicon and Intel |
| `sky-la_<version>_x64-setup.exe` | Windows 10 or later, installs for the current user |
| `SHA256SUMS` (+ `.minisig`) | Every bundle's SHA-256, signed with the release key |
| `latest.json` (+ `.minisig`) | The update manifest the app's opt-in check reads |
| `sky-la-sbom.cdx.json` | CycloneDX SBOM of the Rust dependencies |

Each bundle carries the `skyla-mcp` shim beside the app, because the advisors start it (`tauri.release.conf.json`).

## Cutting a release

1. Pick the version. Set it in `Cargo.toml` (`[workspace.package] version`) and `apps/desktop/src-tauri/tauri.conf.json` (`version`).
2. Add a `## <version>` section at the top of `CHANGELOG.md`. Its first paragraph is what the app shows when it finds the update, so write it for users.
3. Run `just ci`, then `just release-dry-run` on Linux. The dry run builds the `.deb` the way CI does and checks `dist-release/SHA256SUMS`.
4. Commit, then tag and push: `git tag v<version> && git push origin v<version>`.
5. When the workflow finishes, read the draft release, try a bundle per platform, and publish it.

`skyla-release check-version` stops the workflow if the tag, the crates, the shell and the changelog disagree.

## Signing (the maintainer's secrets)

None of these live in the repository. Each is optional: without it, that step is skipped and the release says it's unsigned.

| Secret | Used for |
|---|---|
| `SKYLA_RELEASE_KEY`, `SKYLA_RELEASE_KEY_PASSWORD` | The minisign release key (contents of its `.key` file) and its password. Signs `SHA256SUMS` and `latest.json` |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` | A Developer ID Application certificate (base64 `.p12`). The bundler signs with the hardened runtime |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Notarisation (an app-specific password) |
| `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD` | An Authenticode certificate (base64 `.pfx`); SHA-256 digests, timestamped |

### The release key and the app's update check

The app's update check is off by default. It also refuses to run until the build trusts a release key: `TRUSTED_RELEASE_KEYS` in `crates/skyla-app/src/core/update.rs` is empty. To turn it on:

1. Create the key offline: `minisign -G -p sky-la-release.pub -s sky-la-release.key`. Keep the `.key` file and its password safe; anyone holding both can tell every installed copy that an update exists.
2. Store the `.key` file's contents and the password as the two `SKYLA_RELEASE_KEY*` secrets.
3. Put the public key (the second line of the `.pub` file) in `TRUSTED_RELEASE_KEYS`, and ship that build.

A build that trusts a key believes a manifest only when that key signed it and its page is under `https://github.com/xkazm04/sky-la/releases/`. It reports the version and the page, and never downloads or installs anything.

Rule-pack updates work the same way with `TRUSTED_PACK_KEYS` in `crates/skyla-app/src/core/refdata.rs`. Use a separate key, so a pack signer can't announce app releases.

## Reproducibility

Bundles build from the locked dependency graph (`--locked`, `pnpm install --frozen-lockfile`), with actions pinned to commits. They aren't bit-for-bit reproducible yet: code signing, timestamps and the toolchain's paths differ between builds. `SHA256SUMS` lets anyone check that a download is the one the release published.
