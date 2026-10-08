//! `skyla-release <command>`; see the crate docs and `docs/RELEASING.md`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(args: &[String]) -> Result<String, String> {
    let root = root();
    match args {
        [cmd] if cmd == "version" => skyla_release::workspace_version(&root),
        [cmd, tag] if cmd == "check-version" => {
            skyla_release::check_version(&root, tag).map(|v| format!("version {v} agrees"))
        }
        [cmd, target] if cmd == "stage-shim" => {
            let exe = if target.contains("windows") {
                ".exe"
            } else {
                ""
            };
            let from = root.join(format!("target/{target}/release/skyla-mcp{exe}"));
            let dir = root.join("apps/desktop/src-tauri/binaries");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let to = dir.join(format!("skyla-mcp-{target}{exe}"));
            std::fs::copy(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
            Ok(format!("staged {}", to.display()))
        }
        [cmd, artifacts, out, tag, date] if cmd == "collect" => {
            let version = skyla_release::check_version(&root, tag)?;
            let files = skyla_release::collect(
                &root,
                Path::new(artifacts),
                Path::new(out),
                &version,
                date,
            )?;
            Ok(files.join("\n"))
        }
        [cmd, files @ ..] if cmd == "sign" && !files.is_empty() => {
            let key = std::env::var("SKYLA_RELEASE_KEY")
                .map_err(|_| "SKYLA_RELEASE_KEY (the minisign secret key) isn't set")?;
            let password = std::env::var("SKYLA_RELEASE_KEY_PASSWORD").ok();
            let mut done = Vec::new();
            for f in files {
                let sig = skyla_release::sign(Path::new(f), &key, password.clone())?;
                done.push(sig.display().to_string());
            }
            Ok(done.join("\n"))
        }
        [cmd, dir] if cmd == "verify-sums" => skyla_release::verify_sums(Path::new(dir))
            .map(|n| format!("{n} files match SHA256SUMS")),
        _ => Err(
            "usage: skyla-release check-version <tag> | stage-shim <target> | \
                  collect <artifacts> <out> <tag> <date> | sign <file>… | verify-sums <dir>"
                .into(),
        ),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("skyla-release: {e}");
            ExitCode::FAILURE
        }
    }
}
