//! The explainable matcher: candidates for a bank line, each with a score
//! that is exactly the sum of named contributions, so the inspector can
//! show why ("VS matches 2026-097 +45, amount equals what's open +30 …").
//!
//! Money in is matched to issued invoices, money out to received ones. A transfer that pays several of a
//! customer's invoices becomes a split. Rules come first: a line a rule
//! books never reaches the scorer. Auto-accept needs the user's opt-in, a
//! score over the threshold, a clear margin over the runner-up, and an
//! allocation that settles every invoice it touches exactly; a split must
//! also name its invoices.

use serde::{Deserialize, Serialize};
use skyla_money::Money;

use crate::normalise::{Normalised, canonical_account, name_similarity};
use crate::rules::{Action, Rule, first_rule};

/// Which way an open item is paid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// An issued invoice: the customer pays us (money in).
    #[default]
    Receivable,
    /// A received invoice: we pay the supplier (money out).
    Payable,
}

/// An invoice still (partly) open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenItem {
    /// Which way it's paid.
    #[serde(default)]
    pub side: Side,
    /// The document id.
    pub id: i64,
    /// Its number, e.g. `2026-097`.
    pub number: String,
    /// The variable symbol it asks for.
    pub vs: Option<String>,
    /// The customer's (or, for a payable, the supplier's) name.
    pub customer: String,
    /// Accounts the customer has paid from before (any spelling).
    pub known_accounts: Vec<String>,
    /// Issue date.
    pub issue_date: String,
    /// Due date.
    pub due_date: Option<String>,
    /// What's still open.
    pub open: Money,
}

/// A named signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    /// The variable symbol equals the invoice's.
    VsExact,
    /// The variable symbol is one digit away (a typo).
    VsNear,
    /// The invoice number appears in the message.
    NumberInMessage,
    /// The amount equals what's open.
    AmountExact,
    /// The amounts of several open invoices add up to the line.
    AmountSplit,
    /// Less than what's open.
    AmountPartial,
    /// More than what's open.
    AmountOver,
    /// The customer has paid from this account before.
    KnownAccount,
    /// The payer's name resembles the customer's.
    NameSimilar,
    /// Paid between issue and a while after the due date.
    DateWindow,
    /// Paid before the invoice existed.
    BeforeIssue,
}

/// One reason, with its points.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contribution {
    /// Which signal.
    pub signal: Signal,
    /// Points (negative ones count against).
    pub points: i32,
    /// In words.
    pub detail: String,
}

/// A proposed settlement of one line against one or more invoices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    /// (invoice id, amount it gets).
    pub allocations: Vec<(i64, Money)>,
    /// The invoices' numbers, for people.
    pub numbers: Vec<String>,
    /// Exactly the sum of the contributions.
    pub score: i32,
    /// Why.
    pub contributions: Vec<Contribution>,
    /// Every invoice it touches is settled in full.
    pub settles_exactly: bool,
}

/// What the matcher proposes for a line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Proposal {
    /// A rule books it.
    Rule {
        /// The rule's id.
        rule_id: u32,
        /// Its name.
        name: String,
        /// What it does.
        action: Action,
    },
    /// Settle invoices.
    Settle {
        /// The best candidate.
        best: Candidate,
    },
    /// Nothing fits; a person decides.
    Unmatched,
}

/// The matcher's answer for one line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    /// The line's key.
    pub key: String,
    /// The proposal.
    pub proposal: Proposal,
    /// Every candidate considered, best first.
    pub candidates: Vec<Candidate>,
    /// Whether it may be accepted without asking.
    pub auto: bool,
    /// Why it wasn't, when it wasn't.
    pub held_because: Option<String>,
}

/// When to accept without asking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// The user's opt-in for invoice settlements (rules opt in one by one).
    pub auto_settle: bool,
    /// The lowest score accepted without asking.
    pub threshold: i32,
    /// How far ahead of the runner-up the best must be.
    pub margin: i32,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            auto_settle: true,
            threshold: 70,
            margin: 20,
        }
    }
}

/// Points per signal. Strong identifiers (the symbol, the exact amount,
/// a known account) carry most of the weight; names and dates only help.
const VS_EXACT: i32 = 45;
const VS_NEAR: i32 = 20;
const NUMBER_IN_MESSAGE: i32 = 35;
const AMOUNT_EXACT: i32 = 30;
const AMOUNT_SPLIT: i32 = 30;
const AMOUNT_PARTIAL: i32 = 5;
const AMOUNT_OVER: i32 = -20;
const KNOWN_ACCOUNT: i32 = 20;
const NAME_MAX: i32 = 15;
const DATE_WINDOW: i32 = 5;
const BEFORE_ISSUE: i32 = -30;
/// The least a candidate must score to be offered at all: one strong
/// signal (a symbol, the number, the exact amount or a known account).
const MIN_CANDIDATE: i32 = 25;
/// Days after the due date a payment still counts as "in the window".
const LATE_DAYS: i64 = 60;

fn digits(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

/// One substitution, or one transposition of neighbours.
fn one_digit_apart(a: &str, b: &str) -> bool {
    if a.len() != b.len() || a == b {
        return false;
    }
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let diff: Vec<usize> = (0..a.len()).filter(|&i| a.get(i) != b.get(i)).collect();
    match diff.as_slice() {
        [_] => true,
        [i, j] if *j == i + 1 => a.get(*i) == b.get(*j) && a.get(*j) == b.get(*i),
        _ => false,
    }
}

fn mentions(message: &str, item: &OpenItem) -> bool {
    let flat: String = message.chars().filter(|c| !c.is_whitespace()).collect();
    let number_digits = digits(&item.number);
    flat.contains(&item.number)
        || (number_digits.len() >= 5 && digits(&flat).contains(&number_digits))
}

/// Contributions that describe the payer and the timing, shared by single
/// and split candidates.
fn party_and_date(line: &Normalised, item: &OpenItem, out: &mut Vec<Contribution>) {
    let known = line.counterparty_account.as_ref().is_some_and(|a| {
        item.known_accounts
            .iter()
            .filter_map(|k| canonical_account(k))
            .any(|k| &k == a)
    });
    if known {
        out.push(Contribution {
            signal: Signal::KnownAccount,
            points: KNOWN_ACCOUNT,
            detail: format!("{} has paid from this account before", item.customer),
        });
    }
    let similarity = name_similarity(&line.counterparty_name, &item.customer);
    if similarity >= 60 {
        let points = i32::try_from(similarity).unwrap_or(0) * NAME_MAX / 100;
        out.push(Contribution {
            signal: Signal::NameSimilar,
            points,
            detail: format!("the payer's name is {similarity} % like {}", item.customer),
        });
    }
    let day = |d: &str| skyla_rules::date::parse(d);
    if let (Some(paid), Some(issued)) = (day(&line.line.booking_date), day(&item.issue_date)) {
        let due = item.due_date.as_deref().and_then(day).unwrap_or(issued);
        if paid < issued {
            out.push(Contribution {
                signal: Signal::BeforeIssue,
                points: BEFORE_ISSUE,
                detail: format!("paid before {} was issued", item.number),
            });
        } else if paid <= due + LATE_DAYS {
            out.push(Contribution {
                signal: Signal::DateWindow,
                points: DATE_WINDOW,
                detail: "paid between issue and the due date (or soon after)".into(),
            });
        }
    }
}

fn single(line: &Normalised, item: &OpenItem) -> Candidate {
    let mut c = Vec::new();
    let amount = Money::new(
        line.line.amount.minor().saturating_abs(),
        line.line.amount.currency(),
    );
    let message = line.line.message.as_deref().unwrap_or("");
    let item_vs = item
        .vs
        .clone()
        .unwrap_or_else(|| digits(&item.number).trim_start_matches('0').to_owned());
    match line.line.vs.as_deref() {
        Some(vs) if vs == item_vs => c.push(Contribution {
            signal: Signal::VsExact,
            points: VS_EXACT,
            detail: format!("VS {vs} is {}'s", item.number),
        }),
        Some(vs) if one_digit_apart(vs, &item_vs) => c.push(Contribution {
            signal: Signal::VsNear,
            points: VS_NEAR,
            detail: format!("VS {vs} is one digit from {item_vs} ({})", item.number),
        }),
        _ => {
            if mentions(message, item) {
                c.push(Contribution {
                    signal: Signal::NumberInMessage,
                    points: NUMBER_IN_MESSAGE,
                    detail: format!("the message names {}", item.number),
                });
            }
        }
    }
    let settles = amount.minor() == item.open.minor();
    if settles {
        c.push(Contribution {
            signal: Signal::AmountExact,
            points: AMOUNT_EXACT,
            detail: format!("the amount equals what's open, {}", item.open.format_cs()),
        });
    } else if amount.minor() < item.open.minor() {
        c.push(Contribution {
            signal: Signal::AmountPartial,
            points: AMOUNT_PARTIAL,
            detail: format!(
                "pays {} of {} open",
                amount.format_cs(),
                item.open.format_cs()
            ),
        });
    } else {
        c.push(Contribution {
            signal: Signal::AmountOver,
            points: AMOUNT_OVER,
            detail: format!(
                "pays {} but only {} is open",
                amount.format_cs(),
                item.open.format_cs()
            ),
        });
    }
    party_and_date(line, item, &mut c);
    let allocated = if amount.minor() <= item.open.minor() {
        amount
    } else {
        item.open
    };
    Candidate {
        allocations: vec![(item.id, allocated)],
        numbers: vec![item.number.clone()],
        score: c.iter().map(|x| x.points).sum(),
        contributions: c,
        settles_exactly: settles,
    }
}

/// Several open invoices of one customer whose open amounts add up to the
/// line, up to four at a time, oldest first.
fn splits(line: &Normalised, items: &[OpenItem]) -> Vec<Candidate> {
    let amount = line.line.amount.minor().saturating_abs();
    let message = line.line.message.as_deref().unwrap_or("");
    let mut customers: Vec<&str> = items.iter().map(|i| i.customer.as_str()).collect();
    customers.sort_unstable();
    customers.dedup();
    let mut out = Vec::new();
    for customer in customers {
        let mut own: Vec<&OpenItem> = items.iter().filter(|i| i.customer == customer).collect();
        own.sort_by(|a, b| a.issue_date.cmp(&b.issue_date).then(a.id.cmp(&b.id)));
        own.truncate(12);
        let n = own.len();
        for mask in 1u32..(1u32 << n) {
            let count = mask.count_ones();
            if !(2..=4).contains(&count) {
                continue;
            }
            let picked: Vec<&OpenItem> = (0..n)
                .filter(|i| mask & (1 << i) != 0)
                .filter_map(|i| own.get(i).copied())
                .collect();
            let sum: i64 = picked.iter().map(|i| i.open.minor()).sum();
            if sum != amount {
                continue;
            }
            let mut c = vec![Contribution {
                signal: Signal::AmountSplit,
                points: AMOUNT_SPLIT,
                detail: format!(
                    "{} open invoices of {customer} add up to the amount",
                    picked.len()
                ),
            }];
            let named: Vec<&str> = picked
                .iter()
                .filter(|i| mentions(message, i))
                .map(|i| i.number.as_str())
                .collect();
            let vs_hit = picked.iter().any(|i| {
                let ivs =
                    i.vs.clone()
                        .unwrap_or_else(|| digits(&i.number).trim_start_matches('0').to_owned());
                line.line.vs.as_deref() == Some(ivs.as_str())
            });
            if named.len() == picked.len() || (vs_hit && named.len() + 1 >= picked.len()) {
                c.push(Contribution {
                    signal: Signal::NumberInMessage,
                    points: NUMBER_IN_MESSAGE,
                    detail: "the payment names every invoice it pays".into(),
                });
            }
            if let Some(first) = picked.first() {
                party_and_date(line, first, &mut c);
            }
            out.push(Candidate {
                allocations: picked.iter().map(|i| (i.id, i.open)).collect(),
                numbers: picked.iter().map(|i| i.number.clone()).collect(),
                score: c.iter().map(|x| x.points).sum(),
                contributions: c,
                settles_exactly: true,
            });
        }
    }
    out
}

/// Proposes what to do with a line.
pub fn suggest(
    line: &Normalised,
    items: &[OpenItem],
    rules: &[Rule],
    policy: &Policy,
) -> Suggestion {
    if let Some(rule) = first_rule(rules, line) {
        return Suggestion {
            key: line.key.clone(),
            proposal: Proposal::Rule {
                rule_id: rule.id,
                name: rule.name.clone(),
                action: rule.then.clone(),
            },
            candidates: Vec::new(),
            auto: rule.auto_accept,
            held_because: (!rule.auto_accept)
                .then(|| format!("rule \"{}\" asks before booking", rule.name)),
        };
    }
    let side = if line.line.amount.minor() > 0 {
        Side::Receivable
    } else {
        Side::Payable
    };
    let same_currency: Vec<OpenItem> = items
        .iter()
        .filter(|i| {
            i.side == side && i.open.currency() == line.line.amount.currency() && i.open.minor() > 0
        })
        .cloned()
        .collect();
    if line.line.amount.minor() == 0 || (side == Side::Payable && same_currency.is_empty()) {
        return Suggestion {
            key: line.key.clone(),
            proposal: Proposal::Unmatched,
            candidates: Vec::new(),
            auto: false,
            held_because: Some("money out with no rule; choose an account or create a rule".into()),
        };
    }
    let mut candidates: Vec<Candidate> = same_currency.iter().map(|i| single(line, i)).collect();
    // Only worth considering when no single invoice is paid exactly.
    if !candidates
        .iter()
        .any(|c| c.settles_exactly && c.score >= policy.threshold)
    {
        candidates.extend(splits(line, &same_currency));
    }
    candidates.retain(|c| c.score >= MIN_CANDIDATE);
    candidates.sort_by(|a, b| b.score.cmp(&a.score).then(a.numbers.cmp(&b.numbers)));
    candidates.truncate(5);
    let Some(best) = candidates.first().cloned() else {
        return Suggestion {
            key: line.key.clone(),
            proposal: Proposal::Unmatched,
            candidates,
            auto: false,
            held_because: Some(if side == Side::Receivable {
                "no open invoice fits".into()
            } else {
                "no received invoice or rule fits; book it to an account or create a rule".into()
            }),
        };
    };
    let runner_up = candidates.get(1).map_or(i32::MIN / 2, |c| c.score);
    let held = if !policy.auto_settle {
        Some("auto-accept is off".to_owned())
    } else if best.score < policy.threshold {
        Some(format!(
            "score {} is below {}",
            best.score, policy.threshold
        ))
    } else if best.score - runner_up < policy.margin {
        Some(format!(
            "{} and {} score too close ({} and {})",
            best.numbers.join(" + "),
            candidates
                .get(1)
                .map(|c| c.numbers.join(" + "))
                .unwrap_or_default(),
            best.score,
            runner_up
        ))
    } else if !best.settles_exactly {
        Some("it would settle an invoice only in part".to_owned())
    } else if best.allocations.len() > 1
        && !best
            .contributions
            .iter()
            .any(|c| c.signal == Signal::NumberInMessage)
    {
        Some("the payment matches several invoices together but doesn't name them".to_owned())
    } else {
        None
    };
    Suggestion {
        key: line.key.clone(),
        auto: held.is_none(),
        held_because: held,
        proposal: Proposal::Settle { best },
        candidates,
    }
}
