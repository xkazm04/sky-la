//! Bank statement parsers, deduplication, statement tie-out, rules and the explainable matcher.
//!
//! WP-17 reads statements: ISO 20022 camt.053, SWIFT MT940, the Czech
//! ABO/GPC fixed-width export, and CSV through column profiles. Every
//! parser takes untrusted bytes and returns a [`Statement`] or a
//! [`BankError`]; none of them panics, which fuzz targets (`fuzz/`) and a
//! corpus replay in the tests hold them to. Amounts are exact minor units.
//!
//! WP-18 adds normalisation, deduplication, tie-out and matching.

#![deny(
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_arithmetic
)]

mod camt;
mod csv_profile;
mod gpc;
mod model;
mod mt940;
mod text;

pub use camt::parse_camt053;
pub use csv_profile::{Amounts, CsvProfile, parse_csv};
pub use gpc::parse_gpc;
pub use model::{Account, BankError, BankLine, Format, Statement};
pub use mt940::parse_mt940;

/// The largest file sky-la reads (a year of a busy account is a few MB).
pub const MAX_BYTES: usize = 32 * 1024 * 1024;

/// Recognises a statement's format from its content, CSV as the fallback
/// for text that is none of the others.
pub fn detect(bytes: &[u8]) -> Format {
    let text = text::decode(bytes.get(..bytes.len().min(64 * 1024)).unwrap_or(bytes));
    let head = text.trim_start();
    if head.starts_with('<') && camt::looks_like(head) {
        Format::Camt053
    } else if mt940::looks_like(&text) {
        Format::Mt940
    } else if gpc::looks_like(&text) {
        Format::Gpc
    } else {
        Format::Csv
    }
}

/// Reads a statement file: detects the format, decodes the text (UTF-8, or
/// Windows-1250), and parses it. CSV needs a profile.
pub fn parse(bytes: &[u8], csv: Option<&CsvProfile>) -> Result<Vec<Statement>, BankError> {
    if bytes.len() > MAX_BYTES {
        return Err(BankError::TooLarge {
            size: bytes.len(),
            limit: MAX_BYTES,
        });
    }
    let text = text::decode(bytes);
    match detect(bytes) {
        Format::Camt053 => parse_camt053(&text),
        Format::Mt940 => parse_mt940(&text),
        Format::Gpc => parse_gpc(&text),
        Format::Csv => match csv {
            Some(profile) => parse_csv(&text, profile),
            None => Err(BankError::Unrecognised),
        },
    }
}
