//! ISO 20022 `camt.053` (BankToCustomerStatement), versions 001.02 to 001.08.
//!
//! The XML is parsed into a DOM with DTDs refused (no entity expansion) and
//! a node limit. Elements are matched by local name, so any 001.xx
//! namespace reads the same way; the root must be a camt.053 `Document`.

use roxmltree::{Document, Node, ParsingOptions};
use skyla_money::{Currency, Money};

use crate::model::{Account, BankError, BankLine, Format, Statement, malformed};
use crate::text::{amount, any_date, non_empty, symbol, symbols_in};

const F: Format = Format::Camt053;

fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
}

fn children<'a, 'i>(node: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children()
        .filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn path<'a, 'i>(node: Node<'a, 'i>, names: &[&str]) -> Option<Node<'a, 'i>> {
    names.iter().try_fold(node, |n, name| child(n, name))
}

fn text(node: Node<'_, '_>, names: &[&str]) -> Option<String> {
    path(node, names).and_then(|n| n.text()).and_then(non_empty)
}

/// True when the XML's root is a camt.053 document.
pub(crate) fn looks_like(head: &str) -> bool {
    head.contains("camt.053") && head.contains("BkToCstmrStmt")
}

/// An amount element with its currency and the credit/debit indicator beside it.
fn signed_amount(holder: Node<'_, '_>, fallback: Currency) -> Option<Money> {
    let amt = child(holder, "Amt")?;
    let currency = amt
        .attribute("Ccy")
        .and_then(|c| Currency::from_code(c).ok())
        .unwrap_or(fallback);
    let value = amount(amt.text()?, '.', currency)?;
    match text(holder, &["CdtDbtInd"]).as_deref() {
        Some("DBIT") => value.checked_neg().ok(),
        Some("CRDT") => Some(value),
        _ => None,
    }
}

fn date_of(node: Option<Node<'_, '_>>) -> Option<String> {
    let n = node?;
    text(n, &["Dt"])
        .or_else(|| text(n, &["DtTm"]))
        .and_then(|d| any_date(&d))
}

fn account_of(holder: Option<Node<'_, '_>>) -> Option<String> {
    let id = path(holder?, &["Id"])?;
    text(id, &["IBAN"]).or_else(|| text(id, &["Othr", "Id"]))
}

fn party_name(holder: Option<Node<'_, '_>>) -> Option<String> {
    let h = holder?;
    text(h, &["Nm"]).or_else(|| text(h, &["Pty", "Nm"]))
}

/// Real statements nest about ten deep; the XML parser recurses per level,
/// so a hostile file with thousands of open tags would exhaust the stack.
const MAX_DEPTH: usize = 64;

/// An upper bound on element nesting, found without parsing: start tags
/// open a level, end tags and `/>` close one; comments, processing
/// instructions, declarations and CDATA are skipped.
fn nesting(xml: &str) -> usize {
    let bytes = xml.as_bytes();
    let (mut depth, mut max, mut i) = (0usize, 0usize, 0usize);
    let find = |from: usize, pat: &[u8]| -> Option<usize> {
        bytes
            .get(from..)?
            .windows(pat.len())
            .position(|w| w == pat)
            .map(|p| p + from)
    };
    while let Some(lt) = find(i, b"<") {
        let rest = bytes.get(lt + 1..).unwrap_or_default();
        let (skip_to, kind): (Option<usize>, u8) = if rest.starts_with(b"!--") {
            (find(lt, b"-->").map(|e| e + 3), b'c')
        } else if rest.starts_with(b"![CDATA[") {
            (find(lt, b"]]>").map(|e| e + 3), b'c')
        } else if rest.starts_with(b"?") || rest.starts_with(b"!") {
            (find(lt, b">").map(|e| e + 1), b'c')
        } else if rest.starts_with(b"/") {
            (find(lt, b">").map(|e| e + 1), b'e')
        } else {
            let end = find(lt, b">");
            let empty = end.is_some_and(|e| e > 0 && bytes.get(e - 1) == Some(&b'/'));
            (end.map(|e| e + 1), if empty { b'c' } else { b's' })
        };
        match kind {
            b's' => {
                depth += 1;
                max = max.max(depth);
            }
            b'e' => depth = depth.saturating_sub(1),
            _ => {}
        }
        match skip_to {
            Some(next) => i = next,
            None => break,
        }
    }
    max
}

/// Reads every statement in a camt.053 file.
pub fn parse_camt053(xml: &str) -> Result<Vec<Statement>, BankError> {
    if nesting(xml) > MAX_DEPTH {
        return Err(malformed(
            F,
            None,
            format!("elements nest more than {MAX_DEPTH} deep"),
        ));
    }
    let options = ParsingOptions {
        allow_dtd: false,
        nodes_limit: 2_000_000,
        ..ParsingOptions::default()
    };
    let doc = Document::parse_with_options(xml, options)
        .map_err(|e| malformed(F, None, format!("not well-formed XML: {e}")))?;
    let root = doc.root_element();
    let ns = root.tag_name().namespace().unwrap_or("");
    if root.tag_name().name() != "Document" || !ns.contains("camt.053") {
        return Err(malformed(F, None, "the root isn't a camt.053 Document"));
    }
    let body =
        child(root, "BkToCstmrStmt").ok_or_else(|| malformed(F, None, "no BkToCstmrStmt"))?;
    let mut out = Vec::new();
    for stmt in children(body, "Stmt") {
        out.push(statement(stmt)?);
    }
    if out.is_empty() {
        return Err(malformed(F, None, "no Stmt in the file"));
    }
    Ok(out)
}

fn statement(stmt: Node<'_, '_>) -> Result<Statement, BankError> {
    let acct = child(stmt, "Acct");
    let currency_code = acct.and_then(|a| text(a, &["Ccy"]));
    let currency = match currency_code.as_deref() {
        Some(c) => Currency::from_code(c)
            .map_err(|_| malformed(F, None, format!("unknown currency {c}")))?,
        None => {
            // Fall back to the first amount's currency.
            stmt.descendants()
                .find(|n| n.tag_name().name() == "Amt")
                .and_then(|n| n.attribute("Ccy"))
                .and_then(|c| Currency::from_code(c).ok())
                .ok_or_else(|| malformed(F, None, "the statement names no currency"))?
        }
    };
    let mut warnings = Vec::new();
    let (mut opening, mut closing) = (None, None);
    for bal in children(stmt, "Bal") {
        let code = text(bal, &["Tp", "CdOrPrtry", "Cd"]);
        let value = signed_amount(bal, currency);
        match code.as_deref() {
            Some("OPBD" | "PRCD") if opening.is_none() => opening = value,
            Some("CLBD") => closing = value,
            _ => {}
        }
    }
    let mut lines = Vec::new();
    for (i, ntry) in children(stmt, "Ntry").enumerate() {
        let n = i + 1;
        let status = text(ntry, &["Sts"]).or_else(|| text(ntry, &["Sts", "Cd"]));
        if status.as_deref().is_some_and(|s| s != "BOOK") {
            warnings.push(format!(
                "entry {n} is {} (not booked) and was skipped",
                status.unwrap_or_default()
            ));
            continue;
        }
        let amount = signed_amount(ntry, currency).ok_or_else(|| {
            malformed(
                F,
                Some(n),
                "an entry without a readable amount and credit/debit indicator",
            )
        })?;
        if amount.currency() != currency {
            return Err(malformed(
                F,
                Some(n),
                "an entry in another currency than the account",
            ));
        }
        let booking_date = date_of(child(ntry, "BookgDt"))
            .or_else(|| date_of(child(ntry, "ValDt")))
            .ok_or_else(|| malformed(F, Some(n), "an entry without a booking date"))?;
        let tx = path(ntry, &["NtryDtls", "TxDtls"]);
        let parties = tx.and_then(|t| child(t, "RltdPties"));
        let incoming = amount.minor() >= 0;
        let (name, account) = match parties {
            Some(p) if incoming => (
                party_name(child(p, "Dbtr")),
                account_of(child(p, "DbtrAcct")),
            ),
            Some(p) => (
                party_name(child(p, "Cdtr")),
                account_of(child(p, "CdtrAcct")),
            ),
            None => (None, None),
        };
        let unstructured: Vec<String> = tx
            .and_then(|t| child(t, "RmtInf"))
            .map(|r| {
                children(r, "Ustrd")
                    .filter_map(|u| u.text())
                    .filter_map(non_empty)
                    .collect()
            })
            .unwrap_or_default();
        let message = if unstructured.is_empty() {
            text(ntry, &["AddtlNtryInf"]).or_else(|| tx.and_then(|t| text(t, &["AddtlTxInf"])))
        } else {
            Some(unstructured.join(" "))
        };
        // Czech banks carry the symbols in the end-to-end id, the creditor
        // reference or the free text.
        let end_to_end = tx.and_then(|t| text(t, &["Refs", "EndToEndId"]));
        let creditor_ref = tx.and_then(|t| text(t, &["RmtInf", "Strd", "CdtrRefInf", "Ref"]));
        let haystack = [
            end_to_end.as_deref(),
            creditor_ref.as_deref(),
            message.as_deref(),
            text(ntry, &["AddtlNtryInf"]).as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        let (mut vs, ks, ss) = symbols_in(&haystack);
        if vs.is_none() {
            vs = creditor_ref
                .as_deref()
                .filter(|r| r.chars().all(|c| c.is_ascii_digit()))
                .and_then(symbol);
        }
        lines.push(BankLine {
            sequence: u32::try_from(lines.len() + 1).unwrap_or(u32::MAX),
            booking_date,
            value_date: date_of(child(ntry, "ValDt")),
            amount,
            reversal: text(ntry, &["RvslInd"]).as_deref() == Some("true"),
            counterparty_name: name,
            counterparty_account: account,
            vs,
            ks,
            ss,
            message,
            bank_ref: text(ntry, &["AcctSvcrRef"])
                .or_else(|| tx.and_then(|t| text(t, &["Refs", "AcctSvcrRef"])))
                .or_else(|| text(ntry, &["NtryRef"])),
        });
    }
    let iban = acct.and_then(|a| text(a, &["Id", "IBAN"]));
    let other = acct.and_then(|a| text(a, &["Id", "Othr", "Id"]));
    Ok(Statement {
        format: F,
        account: Account {
            iban,
            number: other,
            bank_code: None,
        },
        number: text(stmt, &["ElctrncSeqNb"]).or_else(|| text(stmt, &["Id"])),
        from: text(stmt, &["FrToDt", "FrDtTm"]).and_then(|d| any_date(&d)),
        to: text(stmt, &["FrToDt", "ToDtTm"]).and_then(|d| any_date(&d)),
        opening,
        closing,
        lines,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_nesting_without_parsing() {
        assert_eq!(nesting("<?xml?><!-- <a><b> --><a><b/><c>x</c></a>"), 2);
        assert_eq!(nesting("<a><![CDATA[<b><c>]]></a>"), 1);
        let deep = "<a>".repeat(10_000);
        assert!(
            parse_camt053(&deep)
                .expect_err("deep")
                .to_string()
                .contains("nest")
        );
    }
}
