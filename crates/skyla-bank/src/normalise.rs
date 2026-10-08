//! Lines in one comparable form: canonical accounts, names without
//! diacritics or legal forms, and a stable key for deduplication.

use serde::{Deserialize, Serialize};

use crate::model::BankLine;

/// `CZ65 0800 …` → `CZ6508000000192000145399`; `19-2000145399/0800` and
/// `000019-2000145399/0800` → `19-2000145399/0800`; a Czech IBAN becomes
/// its domestic form so both spellings of one account compare equal.
pub fn canonical_account(text: &str) -> Option<String> {
    let compact: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase();
    if compact.is_empty() {
        return None;
    }
    if compact.starts_with("CZ")
        && compact.len() == 24
        && compact.chars().skip(2).all(|c| c.is_ascii_digit())
    {
        let bank = compact.get(4..8)?;
        let prefix = compact.get(8..14)?.trim_start_matches('0');
        let number = compact.get(14..24)?.trim_start_matches('0');
        return Some(if prefix.is_empty() {
            format!("{number}/{bank}")
        } else {
            format!("{prefix}-{number}/{bank}")
        });
    }
    if let Some((acct, bank)) = compact.split_once('/') {
        let (prefix, number) = acct.split_once('-').unwrap_or(("", acct));
        let prefix = prefix.trim_start_matches('0');
        let number = number.trim_start_matches('0');
        if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
            return Some(compact);
        }
        return Some(if prefix.is_empty() {
            format!("{number}/{bank}")
        } else {
            format!("{prefix}-{number}/{bank}")
        });
    }
    Some(compact)
}

fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'Á' | 'Ä' => 'a',
        'č' | 'ç' | 'Č' => 'c',
        'ď' | 'Ď' => 'd',
        'é' | 'ě' | 'è' | 'ë' | 'É' | 'Ě' => 'e',
        'í' | 'î' | 'Í' => 'i',
        'ľ' | 'ĺ' | 'Ľ' => 'l',
        'ň' | 'Ň' => 'n',
        'ó' | 'ô' | 'ö' | 'Ó' | 'Ö' => 'o',
        'ř' | 'Ř' => 'r',
        'š' | 'Š' => 's',
        'ť' | 'Ť' => 't',
        'ú' | 'ů' | 'ü' | 'Ú' | 'Ů' | 'Ü' => 'u',
        'ý' | 'Ý' => 'y',
        'ž' | 'Ž' => 'z',
        c => c.to_ascii_lowercase(),
    }
}

const LEGAL_FORMS: [&str; 14] = [
    "s r o",
    "spol s r o",
    "a s",
    "k s",
    "v o s",
    "z s",
    "o p s",
    "se",
    "gmbh",
    "ltd",
    "inc",
    "sro",
    "as",
    "spol",
];

/// `Northwind Traders, s.r.o.` → `northwind traders`.
pub fn canonical_name(text: &str) -> String {
    let folded: String = text
        .chars()
        .map(fold)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect();
    let mut name = folded.split_whitespace().collect::<Vec<_>>().join(" ");
    loop {
        let before = name.clone();
        for form in LEGAL_FORMS {
            if let Some(stripped) = name.strip_suffix(form)
                && stripped.ends_with(' ')
            {
                name = stripped.trim_end().to_owned();
            }
        }
        if name == before {
            break;
        }
    }
    name
}

/// How alike two names are, 0 to 100 (Dice over character bigrams of the
/// canonical names).
pub fn name_similarity(a: &str, b: &str) -> u32 {
    let (a, b) = (canonical_name(a), canonical_name(b));
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    if a == b {
        return 100;
    }
    let grams = |s: &str| -> Vec<(char, char)> {
        let chars: Vec<char> = s.chars().collect();
        chars
            .windows(2)
            .filter_map(|w| Some((*w.first()?, *w.get(1)?)))
            .collect()
    };
    let (ga, mut gb) = (grams(&a), grams(&b));
    let total = ga.len() + gb.len();
    if total == 0 {
        return 0;
    }
    let mut shared = 0usize;
    for g in &ga {
        if let Some(i) = gb.iter().position(|x| x == g) {
            gb.swap_remove(i);
            shared += 1;
        }
    }
    u32::try_from(shared * 200 / total).unwrap_or(0)
}

/// A line ready to compare and to deduplicate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Normalised {
    /// The line as parsed.
    pub line: BankLine,
    /// The statement's account, canonical.
    pub account: String,
    /// The other party's account, canonical.
    pub counterparty_account: Option<String>,
    /// The other party's name, canonical.
    pub counterparty_name: String,
    /// What identifies the line across imports.
    pub key: String,
}

/// Normalises a statement's lines. Lines identical in every field on one
/// day (two equal card payments) get an occurrence number, so they stay two
/// lines and an overlapping re-import still recognises both.
pub fn normalise(account: &str, lines: &[BankLine]) -> Vec<Normalised> {
    let account = canonical_account(account).unwrap_or_default();
    let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    lines
        .iter()
        .map(|l| {
            let counterparty_account = l
                .counterparty_account
                .as_deref()
                .and_then(canonical_account);
            let base = [
                account.as_str(),
                l.booking_date.as_str(),
                &l.amount.minor().to_string(),
                l.amount.currency().code(),
                l.bank_ref.as_deref().unwrap_or(""),
                counterparty_account.as_deref().unwrap_or(""),
                l.vs.as_deref().unwrap_or(""),
                l.message.as_deref().unwrap_or(""),
            ]
            .join("\u{1f}");
            let n = seen.entry(base.clone()).or_insert(0);
            *n += 1;
            Normalised {
                line: l.clone(),
                account: account.clone(),
                counterparty_name: canonical_name(l.counterparty_name.as_deref().unwrap_or("")),
                counterparty_account,
                key: format!("{base}\u{1f}{n}"),
            }
        })
        .collect()
}

/// Splits lines into new ones and ones already imported (`known` keys).
pub fn dedupe<'a>(
    lines: &'a [Normalised],
    known: &std::collections::HashSet<String>,
) -> (Vec<&'a Normalised>, Vec<&'a Normalised>) {
    lines.iter().partition(|l| !known.contains(&l.key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_and_names_compare_in_one_form() {
        assert_eq!(
            canonical_account("CZ65 0800 0000 1920 0014 5399").as_deref(),
            Some("19-2000145399/0800")
        );
        assert_eq!(
            canonical_account("000019-2000145399/0800").as_deref(),
            Some("19-2000145399/0800")
        );
        assert_eq!(
            canonical_account("987654321/0100").as_deref(),
            Some("987654321/0100")
        );
        assert_eq!(
            canonical_name("Northwind Traders, s.r.o."),
            "northwind traders"
        );
        assert_eq!(
            canonical_name("KANCELÁŘE KORUNNÍ spol. s r.o."),
            "kancelare korunni"
        );
        assert_eq!(
            name_similarity("Acme Analytics a.s.", "ACME ANALYTICS"),
            100
        );
        assert!(name_similarity("Northwind Traders s.r.o.", "Northwind Trading") >= 70);
        assert!(name_similarity("Northwind Traders", "Studio Brno") < 20);
    }
}
