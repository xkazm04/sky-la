//! Recurring invoices: a template and a schedule that produce drafts, or
//! issued invoices when the user opts in.
//!
//! Runs are idempotent: each occurrence is recorded with the document it
//! produced, so running twice on a day, or catching up after the app was
//! closed, never makes a second document. A catch-up issues with the actual
//! run date as the issue date (an issue date is never backdated); the
//! occurrence becomes the tax point. Monthly schedules anchored on the 29th
//! to 31st fall on the last day of shorter months.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use skyla_rules::{Pack, date};

use crate::documents::atomically;
use crate::{Accounts, DocKind, DraftInput, InvoicingError, create_draft_as, issue};

/// How often a template runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    /// Every `interval` weeks.
    Weekly,
    /// Every `interval` months.
    Monthly,
    /// Every `interval` quarters.
    Quarterly,
    /// Every `interval` years.
    Yearly,
}

impl Frequency {
    fn as_str(self) -> &'static str {
        match self {
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Quarterly => "quarterly",
            Self::Yearly => "yearly",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "weekly" => Self::Weekly,
            "monthly" => Self::Monthly,
            "quarterly" => Self::Quarterly,
            "yearly" => Self::Yearly,
            _ => return None,
        })
    }
}

/// When a template runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    /// The unit.
    pub frequency: Frequency,
    /// Units between runs, at least 1.
    pub interval: u32,
    /// The first occurrence; its day of month anchors monthly schedules.
    pub start: String,
    /// The last day an occurrence may fall on.
    pub end: Option<String>,
}

/// A template as entered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateInput {
    /// For people, e.g. "Hosting · Northwind".
    pub name: String,
    /// The invoice to produce. Line descriptions and the note may use
    /// `{MM}`, `{YYYY}` and `{month}` (the occurrence's month, in Czech).
    pub draft: DraftInput,
    /// When.
    pub schedule: Schedule,
    /// Days from the issue date to the due date.
    pub due_days: u16,
    /// Issue (number and post) instead of leaving a draft.
    pub auto_issue: bool,
}

/// A stored template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Template {
    /// Row id.
    pub id: i64,
    /// As entered.
    pub input: TemplateInput,
    /// Paused templates don't run.
    pub active: bool,
    /// The next occurrence not yet run, if any.
    pub next: Option<String>,
}

/// What one occurrence produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    /// The template.
    pub template_id: i64,
    /// The scheduled date.
    pub occurrence: String,
    /// The document produced.
    pub document_id: i64,
    /// Its number, when issued.
    pub number: Option<String>,
    /// Why auto-issue failed (the draft is kept for the user to fix).
    pub problem: Option<String>,
}

/// The `k`-th occurrence (from 0) of a schedule, ignoring its end.
fn nth(s: &Schedule, start: i64, k: i64) -> i64 {
    let step = i64::from(s.interval.max(1));
    let months = match s.frequency {
        Frequency::Weekly => return start + 7 * step * k,
        Frequency::Monthly => step * k,
        Frequency::Quarterly => 3 * step * k,
        Frequency::Yearly => 12 * step * k,
    };
    let (y, m, d) = date::from_days(start);
    let total = (m - 1) + months;
    let (year, month) = (y + total.div_euclid(12), total.rem_euclid(12) + 1);
    let next_first = if month == 12 {
        date::to_days(year + 1, 1, 1)
    } else {
        date::to_days(year, month + 1, 1)
    };
    let last = next_first - date::to_days(year, month, 1);
    date::to_days(year, month, d.min(last))
}

/// Every occurrence on or before `through` (and the schedule's end).
pub fn occurrences(s: &Schedule, through: &str) -> Result<Vec<String>, InvoicingError> {
    let bad = |d: &str| InvoicingError::Invalid(vec![format!("{d:?} isn't a YYYY-MM-DD date")]);
    let start = date::parse(&s.start).ok_or_else(|| bad(&s.start))?;
    let mut limit = date::parse(through).ok_or_else(|| bad(through))?;
    if let Some(end) = &s.end {
        limit = limit.min(date::parse(end).ok_or_else(|| bad(end))?);
    }
    let mut out = Vec::new();
    for k in 0.. {
        let d = nth(s, start, k);
        if d > limit {
            break;
        }
        out.push(date::format(d));
    }
    Ok(out)
}

const MONTHS_CS: [&str; 12] = [
    "leden",
    "únor",
    "březen",
    "duben",
    "květen",
    "červen",
    "červenec",
    "srpen",
    "září",
    "říjen",
    "listopad",
    "prosinec",
];

fn fill(text: &str, occurrence: &str) -> String {
    let (y, m) = (&occurrence[..4], &occurrence[5..7]);
    let month = m
        .parse::<usize>()
        .ok()
        .and_then(|i| MONTHS_CS.get(i - 1))
        .copied()
        .unwrap_or("");
    text.replace("{MM}", m)
        .replace("{YYYY}", y)
        .replace("{month}", &format!("{month} {y}"))
}

fn validate(input: &TemplateInput) -> Vec<String> {
    let mut problems = Vec::new();
    if input.name.trim().is_empty() {
        problems.push("a template needs a name".to_owned());
    }
    if input.draft.kind != DocKind::Invoice {
        problems.push("recurring templates produce invoices".to_owned());
    }
    if input.draft.lines.is_empty() {
        problems.push("a template needs at least one line".to_owned());
    }
    if input.schedule.interval == 0 {
        problems.push("the interval is at least 1".to_owned());
    }
    if date::parse(&input.schedule.start).is_none() {
        problems.push(format!(
            "start {:?} isn't a YYYY-MM-DD date",
            input.schedule.start
        ));
    }
    match &input.schedule.end {
        Some(end) if date::parse(end).is_none() => {
            problems.push(format!("end {end:?} isn't a YYYY-MM-DD date"));
        }
        Some(end) if *end < input.schedule.start => {
            problems.push("the end is before the start".to_owned())
        }
        _ => {}
    }
    problems
}

/// Stores a template.
pub fn create_template(conn: &Connection, input: &TemplateInput) -> Result<i64, InvoicingError> {
    let problems = validate(input);
    if !problems.is_empty() {
        return Err(InvoicingError::Invalid(problems));
    }
    let draft =
        serde_json::to_string(&input.draft).map_err(|e| InvoicingError::Rule(e.to_string()))?;
    conn.execute(
        "INSERT INTO recurring_template (name, draft, frequency, interval, start_date, end_date, due_days, auto_issue)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            input.name,
            draft,
            input.schedule.frequency.as_str(),
            input.schedule.interval,
            input.schedule.start,
            input.schedule.end,
            input.due_days,
            input.auto_issue,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Pauses or resumes a template. A resumed template catches up on what it missed.
pub fn set_template_active(conn: &Connection, id: i64, active: bool) -> Result<(), InvoicingError> {
    let n = conn.execute(
        "UPDATE recurring_template SET active = ?2 WHERE id = ?1",
        params![id, active],
    )?;
    if n == 0 {
        return Err(InvoicingError::Invalid(vec![format!(
            "no recurring template {id}"
        )]));
    }
    Ok(())
}

/// Every template, with its next occurrence after `today`'s runs.
pub fn templates(conn: &Connection) -> Result<Vec<Template>, InvoicingError> {
    struct Row {
        id: i64,
        name: String,
        draft: String,
        frequency: String,
        schedule: Schedule,
        due_days: u16,
        auto_issue: bool,
        active: bool,
    }
    let rows: Vec<Row> = conn
        .prepare(
            "SELECT id, name, draft, frequency, interval, start_date, end_date, due_days, auto_issue, active
             FROM recurring_template ORDER BY id",
        )?
        .query_map([], |r| {
            Ok(Row {
                id: r.get(0)?,
                name: r.get(1)?,
                draft: r.get(2)?,
                frequency: r.get(3)?,
                schedule: Schedule {
                    frequency: Frequency::Monthly,
                    interval: r.get(4)?,
                    start: r.get(5)?,
                    end: r.get(6)?,
                },
                due_days: r.get(7)?,
                auto_issue: r.get(8)?,
                active: r.get(9)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for row in rows {
        let Row {
            id,
            name,
            draft,
            frequency,
            mut schedule,
            due_days,
            auto_issue,
            active,
        } = row;
        let draft: DraftInput =
            serde_json::from_str(&draft).map_err(|e| InvoicingError::Rule(e.to_string()))?;
        schedule.frequency = Frequency::parse(&frequency)
            .ok_or_else(|| InvoicingError::Rule(format!("unknown frequency {frequency}")))?;
        let last: Option<String> = conn
            .query_row(
                "SELECT max(occurrence) FROM recurring_run WHERE template_id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let next = next_after(&schedule, last.as_deref())?;
        out.push(Template {
            id,
            input: TemplateInput {
                name,
                draft,
                schedule,
                due_days,
                auto_issue,
            },
            active,
            next,
        });
    }
    Ok(out)
}

fn next_after(s: &Schedule, last: Option<&str>) -> Result<Option<String>, InvoicingError> {
    let start = date::parse(&s.start).unwrap_or_default();
    let end = s.end.as_deref().and_then(date::parse);
    let after = last.and_then(date::parse).unwrap_or(i64::MIN);
    for k in 0.. {
        let d = nth(s, start, k);
        if end.is_some_and(|e| d > e) {
            return Ok(None);
        }
        if d > after {
            return Ok(Some(date::format(d)));
        }
    }
    Ok(None)
}

/// Runs every active template's occurrences due by `today` that haven't run,
/// oldest first, so auto-issued numbers follow the calendar.
pub fn run_recurring(
    conn: &Connection,
    pack: &Pack,
    accounts: &Accounts,
    today: &str,
) -> Result<Vec<Run>, InvoicingError> {
    let mut due = Vec::new();
    for t in templates(conn)?.into_iter().filter(|t| t.active) {
        for occ in occurrences(&t.input.schedule, today)? {
            let done: bool = conn.query_row(
                "SELECT count(*) > 0 FROM recurring_run WHERE template_id = ?1 AND occurrence = ?2",
                params![t.id, occ],
                |r| r.get(0),
            )?;
            if !done {
                due.push((occ, t.id, t.input.clone()));
            }
        }
    }
    due.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let today_days = date::parse(today)
        .ok_or_else(|| InvoicingError::Invalid(vec![format!("{today:?} isn't a date")]))?;
    let mut runs = Vec::new();
    for (occ, template_id, t) in due {
        let mut draft = t.draft.clone();
        draft.tax_point_date = Some(occ.clone());
        draft.due_date = Some(date::format(today_days + i64::from(t.due_days)));
        draft.note = fill(&draft.note, &occ);
        for l in &mut draft.lines {
            l.description = fill(&l.description, &occ);
        }
        // A stable id per occurrence, so a replayed run makes the same document.
        let uid = uuid::Uuid::new_v5(
            &RECURRING_NAMESPACE,
            format!("{template_id}/{occ}").as_bytes(),
        );
        let document_id = atomically(conn, |tx| {
            let id = create_draft_as(tx, &draft, &uid.to_string())?;
            tx.execute(
                "INSERT INTO recurring_run (template_id, occurrence, document_id) VALUES (?1, ?2, ?3)",
                params![template_id, occ, id],
            )?;
            Ok(id)
        })?;
        let (number, problem) = if t.auto_issue {
            match issue(conn, pack, accounts, document_id, today, None) {
                Ok(i) => (Some(i.number), None),
                Err(e) => (None, Some(e.to_string())),
            }
        } else {
            (None, None)
        };
        runs.push(Run {
            template_id,
            occurrence: occ,
            document_id,
            number,
            problem,
        });
    }
    Ok(runs)
}

/// The namespace of recurring documents' ids (UUID v5 of `template/occurrence`).
const RECURRING_NAMESPACE: uuid::Uuid = uuid::uuid!("2f9c6a1e-7b3d-5c40-8e12-9a6b4d0c3f75");
