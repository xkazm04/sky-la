//! Reminders for overdue invoices: a sequence of steps after the due date,
//! each sent at most once, moved to the next working day by the pack, and
//! stopped by payment or a hold. The last step states the statutory late
//! interest and recovery cost. Texts are drafts for the user to send.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use skyla_money::Money;
use skyla_rules::{Pack, date};

use crate::late_interest::{LateInterest, RepoRate, late_interest};
use crate::spayd::variable_symbol;
use crate::{DocKind, InvoicingError, get, reductions, state};

/// How a reminder reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    /// A polite nudge.
    Friendly,
    /// A clear request with a deadline.
    Firm,
    /// The last notice before recovery.
    Final,
}

/// One step of the sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DunningStep {
    /// Days after the due date.
    pub after_days: u16,
    /// How it reads.
    pub tone: Tone,
    /// Whether it states the late interest and recovery cost.
    pub with_interest: bool,
}

/// The reminder sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DunningPolicy {
    /// In order of `after_days`.
    pub steps: Vec<DunningStep>,
}

impl Default for DunningPolicy {
    /// 3, 14 and 30 days after the due date: friendly, firm, final.
    fn default() -> Self {
        let step = |after_days, tone, with_interest| DunningStep {
            after_days,
            tone,
            with_interest,
        };
        Self {
            steps: vec![
                step(3, Tone::Friendly, false),
                step(14, Tone::Firm, false),
                step(30, Tone::Final, true),
            ],
        }
    }
}

/// A reminder that is due.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DunningNotice {
    /// The invoice.
    pub document_id: i64,
    /// Its number.
    pub number: String,
    /// Who owes.
    pub customer: String,
    /// The step (1-based).
    pub step: u8,
    /// How it reads.
    pub tone: Tone,
    /// When the invoice was due.
    pub due_date: String,
    /// When this step became due (a working day).
    pub scheduled_on: String,
    /// Days past the due date today.
    pub days_overdue: i64,
    /// Still owed.
    pub open: Money,
    /// The statutory interest to today, on steps that state it.
    pub interest: Option<LateInterest>,
    /// Why the interest couldn't be computed (missing reference rates).
    pub interest_problem: Option<String>,
    /// Draft subject and body, Czech.
    pub subject_cs: String,
    /// Draft body, Czech.
    pub body_cs: String,
    /// Draft subject, English.
    pub subject_en: String,
    /// Draft body, English.
    pub body_en: String,
}

/// The stored sequence, or the default.
pub fn policy(conn: &Connection) -> Result<DunningPolicy, InvoicingError> {
    let stored: Option<String> = conn
        .query_row("SELECT policy FROM dunning_policy WHERE id = 1", [], |r| {
            r.get(0)
        })
        .optional()?;
    match stored {
        Some(json) => serde_json::from_str(&json).map_err(|e| InvoicingError::Rule(e.to_string())),
        None => Ok(DunningPolicy::default()),
    }
}

/// Replaces the sequence: one to five steps, strictly later each time.
pub fn set_policy(conn: &Connection, policy: &DunningPolicy) -> Result<(), InvoicingError> {
    let mut problems = Vec::new();
    if policy.steps.is_empty() || policy.steps.len() > 5 {
        problems.push("a sequence has one to five steps".to_owned());
    }
    if policy
        .steps
        .windows(2)
        .any(|w| w[1].after_days <= w[0].after_days)
    {
        problems.push("each step comes later than the one before".to_owned());
    }
    if !problems.is_empty() {
        return Err(InvoicingError::Invalid(problems));
    }
    let json = serde_json::to_string(policy).map_err(|e| InvoicingError::Rule(e.to_string()))?;
    conn.execute(
        "INSERT INTO dunning_policy (id, policy) VALUES (1, ?1)
         ON CONFLICT (id) DO UPDATE SET policy = excluded.policy",
        [json],
    )?;
    Ok(())
}

/// Records that a step went out. Each step goes out once.
pub fn record_reminder(
    conn: &Connection,
    document_id: i64,
    step: u8,
    sent_on: &str,
) -> Result<(), InvoicingError> {
    let sent: Option<String> = conn
        .query_row(
            "SELECT sent_on FROM dunning_event WHERE document_id = ?1 AND step = ?2",
            params![document_id, step],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(on) = sent {
        return Err(InvoicingError::Invalid(vec![format!(
            "reminder {step} for document {document_id} already went out on {on}"
        )]));
    }
    conn.execute(
        "INSERT INTO dunning_event (document_id, step, sent_on) VALUES (?1, ?2, ?3)",
        params![document_id, step, sent_on],
    )?;
    Ok(())
}

/// The reminders sent for a document: (step, date).
pub fn reminders_sent(
    conn: &Connection,
    document_id: i64,
) -> Result<Vec<(u8, String)>, InvoicingError> {
    Ok(conn
        .prepare("SELECT step, sent_on FROM dunning_event WHERE document_id = ?1 ORDER BY step")?
        .query_map([document_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?)
}

/// Pauses reminders for a document.
pub fn hold_reminders(
    conn: &Connection,
    document_id: i64,
    reason: &str,
    since: &str,
) -> Result<(), InvoicingError> {
    conn.execute(
        "INSERT INTO dunning_hold (document_id, reason, since) VALUES (?1, ?2, ?3)
         ON CONFLICT (document_id) DO UPDATE SET reason = excluded.reason, since = excluded.since",
        params![document_id, reason, since],
    )?;
    Ok(())
}

/// Resumes reminders for a document.
pub fn release_reminders(conn: &Connection, document_id: i64) -> Result<(), InvoicingError> {
    conn.execute(
        "DELETE FROM dunning_hold WHERE document_id = ?1",
        [document_id],
    )?;
    Ok(())
}

fn working_day_on_or_after(pack: &Pack, mut day: i64) -> i64 {
    while !pack.is_working_day(&date::format(day)) {
        day += 1;
    }
    day
}

/// The reminders due on `today`: per overdue, unpaid, unheld invoice, the
/// latest step whose day has come and that hasn't gone out. An earlier step
/// that was missed (the app was closed) is skipped, never sent late after a
/// firmer one.
pub fn dunning_queue(
    conn: &Connection,
    pack: &Pack,
    repo: &[RepoRate],
    today: &str,
) -> Result<Vec<DunningNotice>, InvoicingError> {
    let today_days = date::parse(today)
        .ok_or_else(|| InvoicingError::Invalid(vec![format!("{today:?} isn't a date")]))?;
    let policy = policy(conn)?;
    let ids: Vec<i64> = conn
        .prepare(
            "SELECT d.id FROM document d
             WHERE d.status = 'issued' AND d.kind = 'invoice' AND d.due_date IS NOT NULL
               AND d.due_date < ?1
               AND NOT EXISTS (SELECT 1 FROM dunning_hold h WHERE h.document_id = d.id)
             ORDER BY d.due_date, d.id",
        )?
        .query_map([today], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for id in ids {
        let st = state(conn, pack, id)?;
        if st.open.minor() <= 0 {
            continue;
        }
        let doc = get(conn, pack, id)?;
        debug_assert_eq!(doc.kind, DocKind::Invoice);
        let due_date = doc.due_date.clone().unwrap_or_default();
        let due = date::parse(&due_date).unwrap_or(today_days);
        let last_sent = reminders_sent(conn, id)?.last().map_or(0, |(s, _)| *s);
        let mut chosen = None;
        for (i, step) in policy.steps.iter().enumerate() {
            let n = u8::try_from(i + 1).unwrap_or(u8::MAX);
            let on = working_day_on_or_after(pack, due + i64::from(step.after_days));
            if n > last_sent && on <= today_days {
                chosen = Some((n, step.clone(), on));
            }
        }
        let Some((n, step, on)) = chosen else {
            continue;
        };
        let (interest, interest_problem) = if step.with_interest {
            let principal = st.gross.checked_sub(st.advances)?;
            match late_interest(
                pack,
                repo,
                principal,
                &due_date,
                today,
                &reductions(conn, pack, id)?,
            ) {
                Ok(i) => (i, None),
                Err(e) => (None, Some(e.to_string())),
            }
        } else {
            (None, None)
        };
        let number = doc.number.clone().unwrap_or_default();
        let supplier = doc.supplier.as_ref();
        let texts = Texts {
            partly_paid: st.open.minor() < st.gross.checked_sub(st.advances)?.minor(),
            number: &number,
            due: &due_date,
            days: today_days - due,
            open: st.open,
            account: supplier.and_then(|s| s.iban.clone()).unwrap_or_default(),
            vs: variable_symbol(&number).unwrap_or_default(),
            signature: supplier.map(|s| s.name.clone()).unwrap_or_default(),
            interest: interest.as_ref(),
        };
        let (subject_cs, body_cs) = texts.cs(step.tone);
        let (subject_en, body_en) = texts.en(step.tone);
        out.push(DunningNotice {
            document_id: id,
            number,
            customer: doc.customer.name.clone(),
            step: n,
            tone: step.tone,
            due_date,
            scheduled_on: date::format(on),
            days_overdue: today_days - due,
            open: st.open,
            interest,
            interest_problem,
            subject_cs,
            body_cs,
            subject_en,
            body_en,
        });
    }
    Ok(out)
}

struct Texts<'a> {
    partly_paid: bool,
    number: &'a str,
    due: &'a str,
    days: i64,
    open: Money,
    account: String,
    vs: String,
    signature: String,
    interest: Option<&'a LateInterest>,
}

fn cs_date(iso: &str) -> String {
    let (y, m, d) = (&iso[..4], &iso[5..7], &iso[8..10]);
    format!(
        "{}. {}. {y}",
        d.trim_start_matches('0'),
        m.trim_start_matches('0')
    )
}

const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn en_date(iso: &str) -> String {
    let month = iso[5..7]
        .parse::<usize>()
        .ok()
        .and_then(|m| MONTHS_EN.get(m - 1))
        .copied()
        .unwrap_or("");
    format!(
        "{} {month} {}",
        iso[8..10].trim_start_matches('0'),
        &iso[..4]
    )
}

/// `CZK 53,092.00`.
fn en_money(m: Money) -> String {
    let cs = skyla_money::format_amount_cs(m.minor(), m.currency());
    let en: String = cs
        .chars()
        .map(|c| match c {
            ',' => '.',
            '\u{a0}' => ',',
            c => c,
        })
        .collect();
    format!("{} {en}", m.currency().code())
}

fn percent_cs(rate: skyla_money::Rate) -> String {
    rate.normalize().to_string().replace('.', ",")
}

impl Texts<'_> {
    fn payment_cs(&self) -> String {
        let mut s = format!("Zbývá uhradit {}", self.open.format_cs());
        if !self.account.is_empty() {
            s.push_str(&format!(" na účet {}", self.account));
        }
        if !self.vs.is_empty() {
            s.push_str(&format!(", variabilní symbol {}", self.vs));
        }
        s.push('.');
        s
    }

    fn payment_en(&self) -> String {
        let mut s = format!("{} remains to be paid", en_money(self.open));
        if !self.account.is_empty() {
            s.push_str(&format!(" to account {}", self.account));
        }
        if !self.vs.is_empty() {
            s.push_str(&format!(", variable symbol {}", self.vs));
        }
        s.push('.');
        s
    }

    fn cs(&self, tone: Tone) -> (String, String) {
        let due = cs_date(self.due);
        let pay = self.payment_cs();
        let sig = &self.signature;
        let n = self.number;
        match tone {
            Tone::Friendly => (
                format!("Připomínka: faktura {n} je po splatnosti"),
                format!(
                    "Dobrý den,\n\nrádi bychom připomněli fakturu {n} se splatností {due}, kterou u nás zatím nevidíme uhrazenou{}. {pay}\n\nPokud jste již platili, považujte prosím tuto zprávu za bezpředmětnou.\n\nS pozdravem\n{sig}",
                    if self.partly_paid {
                        " v plné výši"
                    } else {
                        ""
                    }
                ),
            ),
            Tone::Firm => (
                format!("Upomínka: faktura {n}"),
                format!(
                    "Dobrý den,\n\nfaktura {n} se splatností {due} je {} dní po splatnosti. {pay} Prosíme o úhradu do 7 dnů.\n\nS pozdravem\n{sig}",
                    self.days
                ),
            ),
            Tone::Final => {
                let mut body = format!(
                    "Dobrý den,\n\nfaktura {n} se splatností {due} je {} dní po splatnosti a ani po předchozích upomínkách nebyla uhrazena. {pay}",
                    self.days
                );
                if let Some(i) = self.interest {
                    body.push_str(&format!(
                        "\n\nK dnešnímu dni činí zákonný úrok z prodlení {} (sazba {} % ročně podle nařízení vlády č. 351/2013 Sb.). Dále máme nárok na náklady spojené s uplatněním pohledávky ve výši {}.",
                        i.total.format_cs(),
                        percent_cs(i.annual_rate),
                        i.recovery_cost.format_cs()
                    ));
                }
                body.push_str(&format!(
                    "\n\nPokud nebude dlužná částka uhrazena do 7 dnů, budeme nuceni ji vymáhat.\n\nS pozdravem\n{sig}"
                ));
                (format!("Poslední upomínka: faktura {n}"), body)
            }
        }
    }

    fn en(&self, tone: Tone) -> (String, String) {
        let due = en_date(self.due);
        let pay = self.payment_en();
        let sig = &self.signature;
        let n = self.number;
        match tone {
            Tone::Friendly => (
                format!("Reminder: invoice {n} is overdue"),
                format!(
                    "Hello,\n\nthis is a friendly reminder that invoice {n}, due on {due}, hasn't {} yet. {pay}\n\nIf you have already paid, please disregard this message.\n\nKind regards\n{sig}",
                    if self.partly_paid {
                        "been paid in full"
                    } else {
                        "reached us"
                    }
                ),
            ),
            Tone::Firm => (
                format!("Payment reminder: invoice {n}"),
                format!(
                    "Hello,\n\ninvoice {n}, due on {due}, is {} days overdue. {pay} Please pay within 7 days.\n\nKind regards\n{sig}",
                    self.days
                ),
            ),
            Tone::Final => {
                let mut body = format!(
                    "Hello,\n\ninvoice {n}, due on {due}, is {} days overdue and remains unpaid after our earlier reminders. {pay}",
                    self.days
                );
                if let Some(i) = self.interest {
                    body.push_str(&format!(
                        "\n\nStatutory late interest to date is {} (an annual rate of {} % under Czech Government Regulation No. 351/2013 Coll.). We are also entitled to {} as the cost of recovering the debt.",
                        en_money(i.total),
                        i.annual_rate.normalize(),
                        en_money(i.recovery_cost)
                    ));
                }
                body.push_str(&format!(
                    "\n\nIf the amount isn't paid within 7 days, we will have to pursue it.\n\nKind regards\n{sig}"
                ));
                (format!("Final notice: invoice {n}"), body)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skyla_money::Currency;

    #[test]
    fn formats_for_english_readers() {
        assert_eq!(
            en_money(Money::new(5_309_200, Currency::CZK)),
            "CZK 53,092.00"
        );
        assert_eq!(
            en_money(Money::new(-120_000, Currency::CZK)),
            "CZK -1,200.00"
        );
        assert_eq!(en_date("2026-09-04"), "4 September 2026");
        assert_eq!(cs_date("2026-09-04"), "4. 9. 2026");
    }
}
