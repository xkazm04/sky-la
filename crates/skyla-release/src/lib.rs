//! Release tooling (WP-33), used by `.github/workflows/release.yml` and
//! `just release-dry-run`. It checks that the tag, the crates, the shell
//! and the changelog agree on the version; collects the bundles with a
//! `SHA256SUMS`; writes the update manifest the app's opt-in check reads
//! (`latest.json`); and signs files with the maintainer's minisign key.

use std::fmt::Write as _;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Where releases live; the app's update check trusts pages under it only.
pub const RELEASES: &str = "https://github.com/xkazm04/sky-la/releases/";

/// File endings of the bundles a release ships.
pub const BUNDLES: &[&str] = &[".deb", ".AppImage", ".dmg", "-setup.exe", ".msi"];

/// What went wrong, for people.
pub type Error = String;

/// The workspace version, from `[workspace.package]` in `Cargo.toml`.
pub fn workspace_version(root: &Path) -> Result<String, Error> {
    let text = fs::read_to_string(root.join("Cargo.toml")).map_err(|e| e.to_string())?;
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[workspace.package]";
        } else if in_package && let Some(v) = line.strip_prefix("version") {
            return Ok(v
                .trim_start_matches([' ', '='])
                .trim()
                .trim_matches('"')
                .to_owned());
        }
    }
    Err("Cargo.toml has no [workspace.package] version".into())
}

/// The version the desktop shell bundles as.
pub fn shell_version(root: &Path) -> Result<String, Error> {
    let conf: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("apps/desktop/src-tauri/tauri.conf.json"))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    conf["version"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "tauri.conf.json has no version".into())
}

/// The changelog's notes for `version`: the paragraph under its heading.
pub fn notes(root: &Path, version: &str) -> Result<String, Error> {
    let text = fs::read_to_string(root.join("CHANGELOG.md")).map_err(|e| e.to_string())?;
    let heading = format!("## {version}");
    let mut lines = text.lines().skip_while(|l| !l.starts_with(&heading));
    if lines.next().is_none() {
        return Err(format!("CHANGELOG.md has no \"{heading}\" section"));
    }
    let body: Vec<&str> = lines
        .take_while(|l| !l.starts_with("## "))
        .skip_while(|l| l.trim().is_empty())
        .take_while(|l| !l.trim().is_empty())
        .collect();
    if body.is_empty() {
        return Err(format!("CHANGELOG.md's {version} section is empty"));
    }
    Ok(body.join(" "))
}

/// The tag, the crates, the shell and the changelog agree.
pub fn check_version(root: &Path, tag: &str) -> Result<String, Error> {
    let crates = workspace_version(root)?;
    let shell = shell_version(root)?;
    if crates != shell {
        return Err(format!(
            "Cargo.toml says {crates}, tauri.conf.json says {shell}"
        ));
    }
    if tag != format!("v{crates}") {
        return Err(format!("the tag {tag} isn't v{crates}"));
    }
    notes(root, &crates)?;
    Ok(crates)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn bundles_in(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), Error> {
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() && !path.extension().is_some_and(|e| e == "app") {
            bundles_in(&path, out)?;
        } else if path.is_file()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
            && BUNDLES.iter().any(|b| name.ends_with(b))
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Copies every bundle under `artifacts` into `out`, then writes
/// `SHA256SUMS` and `latest.json` for `version`, published on `date`.
/// Returns the files written.
pub fn collect(
    root: &Path,
    artifacts: &Path,
    out: &Path,
    version: &str,
    date: &str,
) -> Result<Vec<String>, Error> {
    let mut found = Vec::new();
    bundles_in(artifacts, &mut found)?;
    if found.is_empty() {
        return Err(format!("no bundles under {}", artifacts.display()));
    }
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    for path in found {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("a bundle without a name")?
            .to_owned();
        if names.contains(&name) {
            return Err(format!("two bundles are called {name}"));
        }
        fs::copy(&path, out.join(&name)).map_err(|e| e.to_string())?;
        names.push(name);
    }
    names.sort();
    let mut sums = String::new();
    for name in &names {
        let bytes = fs::read(out.join(name)).map_err(|e| e.to_string())?;
        let _ = writeln!(sums, "{}  {name}", sha256_hex(&bytes));
    }
    fs::write(out.join("SHA256SUMS"), sums).map_err(|e| e.to_string())?;
    let manifest = serde_json::json!({
        "version": version,
        "published": date,
        "notes": notes(root, version)?,
        "page": format!("{RELEASES}tag/v{version}"),
    });
    fs::write(out.join("latest.json"), manifest.to_string()).map_err(|e| e.to_string())?;
    names.push("SHA256SUMS".into());
    names.push("latest.json".into());
    Ok(names)
}

/// Signs `file` with a minisign secret key (the `.key` file's contents),
/// writing `file.minisig`.
pub fn sign(file: &Path, secret_key: &str, password: Option<String>) -> Result<PathBuf, Error> {
    let sk = minisign::SecretKeyBox::from_string(secret_key)
        .and_then(|b| b.into_secret_key(password))
        .map_err(|e| format!("the release key doesn't open: {e}"))?;
    let pk = minisign::PublicKey::from_secret_key(&sk).map_err(|e| e.to_string())?;
    let bytes = fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let sig = minisign::sign(Some(&pk), &sk, Cursor::new(bytes), Some(name), None)
        .map_err(|e| e.to_string())?;
    let target = PathBuf::from(format!("{}.minisig", file.display()));
    fs::write(&target, sig.to_string()).map_err(|e| e.to_string())?;
    Ok(target)
}

/// Checks `SHA256SUMS` in `dir` against the files beside it.
pub fn verify_sums(dir: &Path) -> Result<usize, Error> {
    let sums = fs::read_to_string(dir.join("SHA256SUMS")).map_err(|e| e.to_string())?;
    let mut n = 0;
    for line in sums.lines() {
        let (hash, name) = line
            .split_once("  ")
            .ok_or("a SHA256SUMS line without two spaces")?;
        let bytes = fs::read(dir.join(name)).map_err(|e| format!("{name}: {e}"))?;
        if sha256_hex(&bytes) != hash {
            return Err(format!("{name} doesn't match SHA256SUMS"));
        }
        n += 1;
    }
    Ok(n)
}
