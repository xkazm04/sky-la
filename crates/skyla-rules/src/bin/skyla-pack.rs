//! `skyla-pack check <dir>`: validates a rule pack and runs its golden file,
//! for contributors (WP-34). See rules/README.md.

use std::path::Path;
use std::process::ExitCode;

use skyla_rules::{Pack, golden};

fn check(dir: &Path) -> Result<bool, String> {
    let text = std::fs::read_to_string(dir.join("pack.toml"))
        .map_err(|e| format!("{}: {e}", dir.join("pack.toml").display()))?;
    let pack = match Pack::from_toml(&text) {
        Ok(p) => p,
        Err(e) => {
            println!("✗ pack.toml doesn't validate:\n{e}");
            return Ok(false);
        }
    };
    println!(
        "✓ {} ({}, {}): {} values, {} VAT codes, {} obligations, {} holidays, {} acts listed",
        pack.provenance(),
        pack.info.jurisdiction,
        match pack.info.review {
            skyla_rules::Review::Draft => "draft",
            skyla_rules::Review::Reviewed => "reviewed",
        },
        pack.keys().len(),
        pack.vat_codes.len(),
        pack.obligations.len(),
        pack.holidays.len(),
        pack.acts.len()
    );
    let Ok(golden_text) = std::fs::read_to_string(dir.join("golden.toml")) else {
        println!("✗ no golden.toml: every pack needs one (copy rules/_template/golden.toml)");
        return Ok(false);
    };
    let report = golden::run(&pack, &golden_text).map_err(|e| format!("golden.toml: {e}"))?;
    println!("  {} golden cases", report.cases);
    for f in &report.failures {
        println!("✗ {f}");
    }
    for u in &report.uncovered {
        println!("✗ not covered by a golden case: {u}");
    }
    for o in &report.open {
        println!("! open: {o}");
    }
    if report.passed() {
        println!("✓ golden cases pass and cover the pack");
    }
    Ok(report.passed())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [cmd, dir] = args.as_slice() else {
        eprintln!("usage: skyla-pack check <rules/<cc>/<year>>");
        return ExitCode::FAILURE;
    };
    if cmd != "check" {
        eprintln!("usage: skyla-pack check <rules/<cc>/<year>>");
        return ExitCode::FAILURE;
    }
    match check(Path::new(dir)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("skyla-pack: {e}");
            ExitCode::FAILURE
        }
    }
}
