//! WP-34 acceptance (contributor kit): every pack under `rules/`, and the
//! template, parses and passes its golden file with every value, VAT code,
//! obligation and holiday covered. Cases marked `open` are known findings
//! awaiting the maintainer; they're printed, not failed.

use std::path::{Path, PathBuf};

use skyla_rules::Pack;
use skyla_rules::golden;

fn packs() -> Vec<PathBuf> {
    let rules = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules");
    let mut out = vec![rules.join("_template")];
    for country in std::fs::read_dir(&rules).expect("rules/") {
        let country = country.expect("entry").path();
        if !country.is_dir() || country.ends_with("_template") {
            continue;
        }
        for year in std::fs::read_dir(&country).expect("country") {
            let year = year.expect("entry").path();
            if year.join("pack.toml").is_file() {
                out.push(year);
            }
        }
    }
    out
}

#[test]
fn every_pack_passes_its_golden_file_with_full_coverage() {
    let all = packs();
    assert!(
        all.len() >= 2,
        "the template and at least one real pack: {all:?}"
    );
    for dir in all {
        let pack = Pack::from_toml(&std::fs::read_to_string(dir.join("pack.toml")).expect("pack"))
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        let golden = std::fs::read_to_string(dir.join("golden.toml"))
            .unwrap_or_else(|_| panic!("{} has no golden.toml", dir.display()));
        let report = golden::run(&pack, &golden).expect("golden.toml parses");
        for open in &report.open {
            println!("{}: open: {open}", dir.display());
        }
        assert!(
            report.passed(),
            "{}: failures {:#?}, uncovered {:#?}",
            dir.display(),
            report.failures,
            report.uncovered
        );
        assert!(report.cases > 0);
    }
}

#[test]
fn the_harness_catches_a_wrong_value_a_gap_and_a_stale_open_note() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/_template");
    let pack = Pack::from_toml(&std::fs::read_to_string(dir.join("pack.toml")).expect("pack"))
        .expect("valid");
    let golden = std::fs::read_to_string(dir.join("golden.toml")).expect("golden");

    let wrong = golden.replacen("expect = \"20\"", "expect = \"21\"", 1);
    let r = golden::run(&pack, &wrong).expect("parses");
    assert_eq!(r.failures.len(), 1, "{:?}", r.failures);

    let gap = golden.replace(
        "[[working_day]]\ndate = \"2026-04-06\"",
        "[[working_day]]\ndate = \"2026-04-08\"",
    );
    let r = golden::run(&pack, &gap).expect("parses");
    assert!(
        r.uncovered.iter().any(|u| u.contains("Easter Monday")),
        "{:?}",
        r.uncovered
    );

    let open = golden.replacen(
        "expect = \"20\"\n",
        "expect = \"21\"\nopen = \"reviewer thinks 21\"\n",
        1,
    );
    let r = golden::run(&pack, &open).expect("parses");
    assert!(r.passed() && r.open.len() == 1, "{r:?}");
    let stale = golden.replacen(
        "expect = \"20\"\n",
        "expect = \"20\"\nopen = \"fixed already\"\n",
        1,
    );
    assert!(!golden::run(&pack, &stale).expect("parses").passed());

    let wrong_deadline = golden.replace("due = \"2026-03-20\"", "due = \"2026-03-21\"");
    assert!(
        !golden::run(&pack, &wrong_deadline)
            .expect("parses")
            .passed()
    );
    assert!(golden::run(&pack, "[[nonsense]]\nx = 1\n").is_err());
}
