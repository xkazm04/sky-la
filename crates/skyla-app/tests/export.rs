//! WP-31 acceptance (export): one zip with the whole journal (JSON and
//! CSV), the chart and every issued document; an independent zip reader
//! opens it, the manifest's hashes match every file, the CSV has a row per
//! posting, and the same books always give the same bytes.

#![allow(clippy::unwrap_used)]

use std::process::Command;

use base64::Engine as _;
use skyla_app::Core;

fn export() -> (skyla_app::dto::ExportDto, Vec<u8>) {
    let e = Core::demo().unwrap().export_books().unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&e.content_base64)
        .unwrap();
    (e, bytes)
}

#[test]
fn the_export_is_a_valid_reproducible_zip_of_everything() {
    let (e, bytes) = export();
    assert_eq!(e.file_name, "sky-la-export-Jan_Novak-2026-10-07.zip");
    assert_eq!(bytes.len() as u32, e.bytes);
    assert_eq!(export().1, bytes, "byte-for-byte reproducible");

    let dir = tempfile::tempdir().unwrap();
    let zip = dir.path().join("export.zip");
    std::fs::write(&zip, &bytes).unwrap();
    // Python's zipfile checks every CRC and lists, extracts and hashes the
    // files, then checks the manifest against them and counts CSV rows.
    let script = r#"
import hashlib, json, sys, zipfile, csv, io
z = zipfile.ZipFile(sys.argv[1])
assert z.testzip() is None, "bad CRC"
names = z.namelist()
m = json.loads(z.read("manifest.json"))
for f in m["files"]:
    data = z.read(f["name"])
    assert hashlib.sha256(data).hexdigest() == f["sha256"], f["name"]
    assert len(data) == f["bytes"], f["name"]
journal = json.loads(z.read("journal.json"))
postings = sum(len(e["lines"]) for e in journal["entries"])
rows = list(csv.DictReader(io.StringIO(z.read("journal.csv").decode())))
assert len(rows) == postings, (len(rows), postings)
debit = sum(round(float(r["debit"] or 0) * 100) for r in rows)
credit = sum(round(float(r["credit"] or 0) * 100) for r in rows)
assert debit == credit, (debit, credit)
assert all(z.read(n)[:5] == b"%PDF-" for n in names if n.endswith(".pdf"))
assert any(n.endswith(".isdoc") for n in names)
print(len(names), len(journal["entries"]), m["chainHead"] is not None)
"#;
    let out = Command::new("python3")
        .arg("-c")
        .arg(script)
        .arg(&zip)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = report.split_whitespace().collect();
    assert_eq!(parts[0].parse::<u32>().unwrap(), e.files);
    assert_eq!(parts[2], "True", "the manifest anchors the chain head");
}
