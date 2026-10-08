//! WP-17 acceptance: zero panics. Replays the committed fuzz corpus (a
//! sample of what `cargo fuzz` grew, plus every crash it ever found) on
//! stable, and mutates the samples at random: every input must come back
//! as a statement or an error.

use proptest::prelude::*;
use skyla_bank::{CsvProfile, parse, parse_camt053, parse_csv, parse_gpc, parse_mt940};

fn files(dir: &str) -> Vec<Vec<u8>> {
    let path = format!("{}/tests/{dir}", env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&path).unwrap_or_else(|e| panic!("{path}: {e}")) {
        let p = entry.expect("entry").path();
        if p.is_file() {
            out.push(std::fs::read(&p).expect("read"));
        }
    }
    out
}

fn every_parser(bytes: &[u8]) {
    let fio = CsvProfile::fio();
    let _ = parse(bytes, Some(&fio));
    if let Ok(text) = std::str::from_utf8(bytes) {
        let _ = parse_camt053(text);
        let _ = parse_mt940(text);
        let _ = parse_gpc(text);
        let _ = parse_csv(text, &fio);
    }
}

#[test]
fn the_committed_corpus_replays_without_a_panic() {
    let mut n = 0;
    for target in ["parse_any", "camt053", "mt940", "gpc", "csv"] {
        for bytes in files(&format!("corpus/{target}")) {
            every_parser(&bytes);
            n += 1;
        }
    }
    assert!(n >= 200, "the corpus holds {n} inputs");
}

fn samples() -> Vec<Vec<u8>> {
    files("samples")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn mutated_samples_never_panic(
        pick in 0usize..7,
        edits in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>(), 0u8..4), 1..12),
    ) {
        let all = samples();
        let mut bytes = all[pick % all.len()].clone();
        for (at, byte, kind) in edits {
            if bytes.is_empty() {
                break;
            }
            let i = at.index(bytes.len());
            match kind {
                0 => bytes[i] = byte,
                1 => { bytes.remove(i); }
                2 => bytes.insert(i, byte),
                _ => bytes.truncate(i),
            }
        }
        every_parser(&bytes);
    }

    #[test]
    fn random_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..2048)) {
        every_parser(&bytes);
    }
}
