//! The full export (WP-31): everything in the books, in open formats, in
//! one zip the user owns. The journal as JSON (every entry with its chain
//! hash) and CSV (one row per posting), the chart of accounts, each issued
//! document as ISDOC and PDF, and a manifest with the chain head and a
//! SHA-256 of every file. The zip is byte-for-byte reproducible.

use std::io::Write;

use flate2::Compression;
use flate2::write::DeflateEncoder;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::Core;
use crate::dto::ExportDto;
use crate::error::CoreError;

/// A minimal zip writer: deflated entries, one fixed timestamp, no extras.
struct Zip {
    out: Vec<u8>,
    central: Vec<u8>,
    count: u16,
    dos_time: u16,
    dos_date: u16,
}

impl Zip {
    fn new(day: &str) -> Self {
        let num =
            |r: std::ops::Range<usize>| day.get(r).and_then(|v| v.parse::<u16>().ok()).unwrap_or(1);
        let (y, m, d) = (num(0..4).max(1980), num(5..7), num(8..10));
        Self {
            out: Vec::new(),
            central: Vec::new(),
            count: 0,
            dos_time: 12 << 11,
            dos_date: ((y - 1980) << 9) | (m << 5) | d,
        }
    }

    fn add(&mut self, name: &str, data: &[u8]) -> Result<(), CoreError> {
        let io = |e: std::io::Error| CoreError::BadRequest(format!("export: {e}"));
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(data).map_err(io)?;
        let packed = enc.finish().map_err(io)?;
        let crc = crc32fast::hash(data);
        let (csize, usize_) = (
            u32::try_from(packed.len())
                .map_err(|_| CoreError::BadRequest("export: a file is too big".into()))?,
            u32::try_from(data.len())
                .map_err(|_| CoreError::BadRequest("export: a file is too big".into()))?,
        );
        let offset = u32::try_from(self.out.len())
            .map_err(|_| CoreError::BadRequest("export: too big".into()))?;
        let name_len = u16::try_from(name.len())
            .map_err(|_| CoreError::BadRequest("export: name too long".into()))?;
        let header = |sig: u32, central: bool| {
            let mut h = Vec::new();
            h.extend(sig.to_le_bytes());
            if central {
                h.extend(20_u16.to_le_bytes()); // made by
            }
            h.extend(20_u16.to_le_bytes()); // version needed
            h.extend(0x0800_u16.to_le_bytes()); // UTF-8 names
            h.extend(8_u16.to_le_bytes()); // deflate
            h.extend(self.dos_time.to_le_bytes());
            h.extend(self.dos_date.to_le_bytes());
            h.extend(crc.to_le_bytes());
            h.extend(csize.to_le_bytes());
            h.extend(usize_.to_le_bytes());
            h.extend(name_len.to_le_bytes());
            h.extend(0_u16.to_le_bytes()); // extra
            if central {
                h.extend(0_u16.to_le_bytes()); // comment
                h.extend(0_u16.to_le_bytes()); // disk
                h.extend(0_u16.to_le_bytes()); // internal attributes
                h.extend(0_u32.to_le_bytes()); // external attributes
                h.extend(offset.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        let local = header(0x0403_4b50, false);
        let central = header(0x0201_4b50, true);
        self.out.extend(local);
        self.out.extend(packed);
        self.central.extend(central);
        self.count += 1;
        Ok(())
    }

    fn finish(mut self) -> Vec<u8> {
        let start = u32::try_from(self.out.len()).unwrap_or(u32::MAX);
        let size = u32::try_from(self.central.len()).unwrap_or(u32::MAX);
        self.out.extend(&self.central);
        self.out.extend(0x0605_4b50_u32.to_le_bytes());
        self.out.extend([0, 0, 0, 0]); // disks
        self.out.extend(self.count.to_le_bytes());
        self.out.extend(self.count.to_le_bytes());
        self.out.extend(size.to_le_bytes());
        self.out.extend(start.to_le_bytes());
        self.out.extend(0_u16.to_le_bytes());
        self.out
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// CSV field: quoted when it needs to be.
fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

fn decimal(minor: i64) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let a = minor.unsigned_abs();
    format!("{sign}{}.{:02}", a / 100, a % 100)
}

/// A portable file name: Czech letters without their marks, anything
/// else outside ASCII letters, digits, `-` and `_` as `_`.
fn file_name(text: &str) -> String {
    const FROM: &str = "áčďéěíňóřšťúůýžÁČĎÉĚÍŇÓŘŠŤÚŮÝŽ";
    const TO: &str = "acdeeinorstuuyzACDEEINORSTUUYZ";
    text.chars()
        .map(|c| {
            let c = FROM
                .chars()
                .position(|f| f == c)
                .and_then(|i| TO.chars().nth(i))
                .unwrap_or(c);
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl Core {
    /// Everything in the books, as one reproducible zip.
    pub fn export_books(&self) -> Result<ExportDto, CoreError> {
        use base64::Engine as _;
        let day = self.domain.entity.as_of.clone();
        let entries = skyla_ledger::list_posted(&self.db(), "1900-01-01", "2999-12-31")?;
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();

        // The journal, JSON.
        let journal: Vec<Value> = entries
            .iter()
            .map(|e| {
                json!({
                    "id": e.id,
                    "uid": e.uid,
                    "date": e.date,
                    "postedSeq": e.posted_seq,
                    "postedAt": e.posted_at,
                    "source": e.source_kind.as_str(),
                    "reference": e.source_ref,
                    "memo": e.memo,
                    "approvedBy": e.approved_by,
                    "reverses": e.reverses_id,
                    "chainHash": e.chain_hash,
                    "lines": e.lines.iter().map(|l| json!({
                        "account": l.account,
                        "amountMinor": l.amount.minor(),
                        "currency": l.amount.currency().code(),
                        "functionalMinor": l.functional.minor(),
                        "fxRate": l.fx_rate,
                        "vatCode": l.vat_code,
                        "memo": l.memo,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        files.push((
            "journal.json".into(),
            serde_json::to_vec_pretty(
                &json!({ "functionalCurrency": self.currency.code(), "entries": journal }),
            )
            .map_err(|e| CoreError::BadRequest(e.to_string()))?,
        ));

        // The journal, CSV: one row per posting.
        let mut rows = String::from(
            "entry,posted_seq,date,account,account_name,debit,credit,currency,amount,fx_rate,vat_code,memo,source,reference\n",
        );
        for e in &entries {
            for l in &e.lines {
                let f = l.functional.minor();
                rows.push_str(
                    &[
                        e.id.to_string(),
                        e.posted_seq.map(|s| s.to_string()).unwrap_or_default(),
                        e.date.clone(),
                        l.account.clone(),
                        csv(&self.account_name(&l.account)),
                        if f >= 0 { decimal(f) } else { String::new() },
                        if f < 0 { decimal(-f) } else { String::new() },
                        l.amount.currency().code().to_owned(),
                        decimal(l.amount.minor()),
                        l.fx_rate.clone().unwrap_or_default(),
                        l.vat_code.clone().unwrap_or_default(),
                        csv(if l.memo.is_empty() { &e.memo } else { &l.memo }),
                        e.source_kind.as_str().to_owned(),
                        csv(e.source_ref.as_deref().unwrap_or("")),
                    ]
                    .join(","),
                );
                rows.push('\n');
            }
        }
        files.push(("journal.csv".into(), rows.into_bytes()));

        // The chart.
        let mut chart = String::from("code,name_cs,name_en,kind,active\n");
        for a in skyla_ledger::list_accounts(&self.db())? {
            chart.push_str(&format!(
                "{},{},{},{},{}\n",
                a.code,
                csv(&a.name_cs),
                csv(&a.name_en),
                a.kind.as_str(),
                a.active
            ));
        }
        files.push(("accounts.csv".into(), chart.into_bytes()));

        // Issued documents: ISDOC and PDF.
        for i in self
            .invoices()?
            .into_iter()
            .filter(|i| i.number.is_some() && i.status != "draft" && i.status != "scheduled")
        {
            let number = file_name(i.number.as_deref().unwrap_or_default());
            let xml = self.invoice_xml(i.id, "isdoc")?;
            files.push((format!("documents/{number}.isdoc"), xml.xml.into_bytes()));
            let pdf = self.invoice_pdf(i.id, "cs")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(pdf.pdf_base64.as_bytes())
                .map_err(|e| CoreError::BadRequest(e.to_string()))?;
            files.push((format!("documents/{number}.pdf"), bytes));
        }

        let head = skyla_ledger::verify_chain(&self.db())
            .ok()
            .and_then(|c| c.head);
        let manifest = json!({
            "format": 1,
            "entity": self.domain.entity.display_name,
            "exportedOn": day,
            "pack": self.pack.provenance(),
            "entries": entries.len(),
            "chainHead": head,
            "files": files.iter().map(|(n, b)| json!({ "name": n, "bytes": b.len(), "sha256": hex(&Sha256::digest(b)) })).collect::<Vec<_>>(),
        });
        let readme = format!(
            "sky-la export of {} on {day}\n\n\
             journal.json  every posted entry with its lines and hash-chain link\n\
             journal.csv   one row per posting (decimal point, comma-separated)\n\
             accounts.csv  the chart of accounts\n\
             documents/    issued documents as ISDOC 6.0.2 and PDF\n\
             manifest.json the chain head and a SHA-256 of every file\n",
            self.domain.entity.display_name
        );
        let mut zip = Zip::new(&day);
        zip.add("README.txt", readme.as_bytes())?;
        zip.add(
            "manifest.json",
            &serde_json::to_vec_pretty(&manifest)
                .map_err(|e| CoreError::BadRequest(e.to_string()))?,
        )?;
        for (name, bytes) in &files {
            zip.add(name, bytes)?;
        }
        let bytes = zip.finish();
        Ok(ExportDto {
            file_name: format!(
                "sky-la-export-{}-{day}.zip",
                file_name(&self.domain.entity.display_name)
            ),
            files: u32::try_from(files.len() + 2).unwrap_or(u32::MAX),
            bytes: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        })
    }
}
