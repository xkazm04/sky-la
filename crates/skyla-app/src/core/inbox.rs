//! Approving the inbox's postings (improvement wave 11). A proposal from a
//! rule or an advisor is only ever a proposal: the user pressing Approve is
//! the approval, and the kernel re-validates the entry and posts it. A
//! proposal about a bank line books that line; one an advisor filed through
//! its tools posts on its own and leaves the inbox.

use skyla_ledger::{NewEntry, NewLine, SourceKind, create_draft_as, post_entry_at};
use skyla_money::Money;

use super::Core;
use super::bank::LineEntry;
use crate::demo::DomainEntry;
use crate::dto::ProposalDto;
use crate::error::CoreError;

const ADVISOR_NAMESPACE: uuid::Uuid = uuid::uuid!("6d1b9e37-4a2c-5f80-9b63-1e7c4a0d8f52");

/// What advisors filed through their tools, waiting for the user. Postings
/// keep the entry as proposed, in minor units, so approving posts exactly
/// what the user reviewed.
#[derive(Default)]
pub(crate) struct AdvisorInbox {
    items: Vec<(ProposalDto, Option<DomainEntry>)>,
    /// How many were ever filed, so an id is never reused after one leaves.
    filed: usize,
}

impl AdvisorInbox {
    /// Files a proposal and returns its id.
    pub(crate) fn file(&mut self, mut proposal: ProposalDto, entry: Option<DomainEntry>) -> String {
        self.filed += 1;
        proposal.id = format!("advisor-{}", self.filed);
        let id = proposal.id.clone();
        self.items.push((proposal, entry));
        id
    }

    pub(crate) fn proposals(&self) -> impl Iterator<Item = &ProposalDto> {
        self.items.iter().map(|(p, _)| p)
    }
}

fn bad(message: impl Into<String>) -> CoreError {
    CoreError::BadRequest(message.into())
}

fn source_kind(kind: &str) -> SourceKind {
    match kind {
        "advisor" => SourceKind::Advisor,
        "rule" => SourceKind::Rule,
        _ => SourceKind::Bank,
    }
}

impl Core {
    fn new_lines(&self, entry: &DomainEntry) -> Vec<NewLine> {
        entry
            .lines
            .iter()
            .map(|l| NewLine {
                vat_code: l.vat_code.clone(),
                // Signed: a negative amount is a credit, as the kernel reads it.
                ..NewLine::debit(&l.account, Money::new(l.amount_minor, self.currency))
            })
            .collect()
    }

    /// Approves postings in the inbox, in order, and returns the inbox as it
    /// is afterwards. Each posts through the kernel or nothing does: the
    /// first refusal stops the rest and is the answer.
    pub fn approve_proposals(&self, ids: &[String]) -> Result<Vec<ProposalDto>, CoreError> {
        if ids.is_empty() {
            return Err(bad("choose a proposal to approve"));
        }
        let shown = self.proposals()?;
        for id in ids {
            let Some(p) = shown.iter().find(|p| &p.id == id) else {
                return Err(bad(format!("{id} isn't in the inbox (approved already?)")));
            };
            if p.kind != "posting" || p.entry.is_none() {
                return Err(bad(format!(
                    "“{}” has no entry to post; book it in Bank",
                    p.title
                )));
            }
            if let Some(d) = self.domain.proposals.iter().find(|d| &d.id == id) {
                self.approve_line_proposal(d)?;
            } else {
                self.approve_advisor_entry(id)?;
            }
        }
        self.persist_bank()?;
        self.proposals()
    }

    /// A rule's or an advisor's proposal about a bank line: books the line.
    fn approve_line_proposal(&self, p: &crate::demo::DomainProposal) -> Result<(), CoreError> {
        let (Some(line), Some(entry)) = (&p.bank_line_id, &p.entry) else {
            return Err(bad(format!("“{}” isn't about a bank line", p.title)));
        };
        // The same checks the inbox showed it with: known accounts, the
        // engine's VAT.
        let proposed = self.proposed_entry(entry)?;
        if !proposed.balanced {
            return Err(bad(format!("“{}” doesn't balance", p.title)));
        }
        let mut state = self.bank_state();
        let items = self.open_items(&state)?;
        let settles = entry
            .settles
            .iter()
            .map(|s| {
                let item = items
                    .iter()
                    .find(|i| i.number == s.invoice)
                    .ok_or_else(|| bad(format!("invoice {} isn't open", s.invoice)))?;
                if s.amount_minor > item.open.minor() {
                    return Err(bad(format!(
                        "{} has only {} open",
                        item.number,
                        item.open.format_cs()
                    )));
                }
                Ok((item.id, Money::new(s.amount_minor, self.currency)))
            })
            .collect::<Result<Vec<_>, CoreError>>()?;
        if Self::find_line(&state, line)?.line.booking_date != entry.date {
            return Err(bad(format!(
                "“{}” is dated {}, not on the bank line's day",
                p.title, entry.date
            )));
        }
        let how = LineEntry {
            source_kind: source_kind(&p.source_kind),
            memo: Some(entry.memo.clone()),
            created_by: if p.source_kind == "advisor" {
                "advisor"
            } else {
                "rule"
            },
            label: format!("Approved: {}", p.title),
        };
        self.post_line_as(&mut state, line, self.new_lines(entry), &settles, how)
    }

    /// An entry an advisor proposed with its tools: posts it on its own.
    fn approve_advisor_entry(&self, id: &str) -> Result<(), CoreError> {
        let mut inbox = self
            .advisor_inbox
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let at = inbox
            .items
            .iter()
            .position(|(p, _)| p.id == id)
            .ok_or_else(|| bad(format!("{id} isn't in the inbox")))?;
        let Some(entry) = inbox.items[at].1.clone() else {
            return Err(bad(format!("{id} has no entry to post")));
        };
        let new = NewEntry {
            date: entry.date.clone(),
            source_kind: SourceKind::Advisor,
            source_ref: Some(id.to_owned()),
            memo: entry.memo.clone(),
            created_by: "advisor".into(),
            lines: self.new_lines(&entry),
        };
        let db = self.db();
        // Unique in these books (the ledger's size is part of it) and the
        // same on every run of a recording.
        let posted: i64 = db
            .query_row("SELECT COUNT(*) FROM journal_entry", [], |r| r.get(0))
            .map_err(|e| bad(e.to_string()))?;
        let uid = uuid::Uuid::new_v5(&ADVISOR_NAMESPACE, format!("{posted}/{id}").as_bytes());
        let posted_at = format!("{}T12:00:00.000Z", self.domain.entity.as_of);
        db.execute_batch("SAVEPOINT approve")
            .map_err(|e| bad(e.to_string()))?;
        let result = create_draft_as(&db, &new, &uid.to_string())
            .and_then(|entry_id| post_entry_at(&db, entry_id, Some("user"), Some(&posted_at)));
        match result {
            Ok(_) => {
                db.execute_batch("RELEASE approve")
                    .map_err(|e| bad(e.to_string()))?;
                inbox.items.remove(at);
                Ok(())
            }
            Err(e) => {
                let _ = db.execute_batch("ROLLBACK TO approve; RELEASE approve");
                Err(e.into())
            }
        }
    }
}
