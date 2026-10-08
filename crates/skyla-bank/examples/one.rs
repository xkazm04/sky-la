//! Runs one parser on one file (debugging fuzz finds).
fn main() {
    let path = std::env::args().nth(1).expect("path");
    let which = std::env::args().nth(2).expect("parser");
    let bytes = std::fs::read(&path).expect("read");
    let text = String::from_utf8_lossy(&bytes);
    match which.as_str() {
        "camt" => {
            let _ = skyla_bank::parse_camt053(&text);
        }
        "mt940" => {
            let _ = skyla_bank::parse_mt940(&text);
        }
        "gpc" => {
            let _ = skyla_bank::parse_gpc(&text);
        }
        "csv" => {
            let _ = skyla_bank::parse_csv(&text, &skyla_bank::CsvProfile::fio());
        }
        _ => {
            let _ = skyla_bank::parse(&bytes, Some(&skyla_bank::CsvProfile::fio()));
        }
    }
}
