//! Recurring invoices for real books (improvement wave 10): a template made
//! from the editor's draft and a schedule, run whenever the books open (and
//! at once when created), so each occurrence becomes a draft, or an issued
//! invoice when the user opted in. Runs are idempotent in the invoicing
//! crate: an occurrence never makes a second document.

use skyla_invoicing::recurring::{Frequency, Schedule, TemplateInput};

use super::Core;
use crate::dto::{RecurringDraftDto, RecurringTemplateDto};
use crate::error::CoreError;

impl Core {
    /// Makes a recurring template from what the editor typed.
    pub fn create_recurring(
        &self,
        r: &RecurringDraftDto,
    ) -> Result<Vec<RecurringTemplateDto>, CoreError> {
        let mut problems = Vec::new();
        let draft = match self.draft_input(&r.draft) {
            Ok(d) => Some(d),
            Err(CoreError::Invoicing(skyla_invoicing::InvoicingError::Invalid(p))) => {
                problems.extend(p);
                None
            }
            Err(e) => return Err(e),
        };
        let frequency = match r.frequency.as_str() {
            "monthly" => Some(Frequency::Monthly),
            "quarterly" => Some(Frequency::Quarterly),
            "yearly" => Some(Frequency::Yearly),
            other => {
                problems.push(format!(
                    "repeat {other:?} isn't monthly, quarterly or yearly"
                ));
                None
            }
        };
        if !(1..=12).contains(&r.interval) {
            problems.push("repeat every 1 to 12 periods".to_owned());
        }
        if !skyla_ledger::is_iso_date(&r.start) {
            problems.push(format!("the first date {:?} isn't a date", r.start));
        }
        let name = if r.name.trim().is_empty() {
            draft
                .as_ref()
                .map(|d| d.customer.name.clone())
                .unwrap_or_default()
        } else {
            r.name.trim().to_owned()
        };
        let (Some(draft), Some(frequency), true) = (draft, frequency, problems.is_empty()) else {
            return Err(CoreError::Invoicing(
                skyla_invoicing::InvoicingError::Invalid(problems),
            ));
        };
        skyla_invoicing::recurring::create_template(
            &self.db(),
            &TemplateInput {
                name,
                draft,
                schedule: Schedule {
                    frequency,
                    interval: r.interval,
                    start: r.start.clone(),
                    end: None,
                },
                due_days: r.draft.due_days,
                auto_issue: r.auto_issue,
            },
        )?;
        self.run_recurring()?;
        self.recurring_templates()
    }

    /// Pauses or resumes a template.
    pub fn set_recurring_active(
        &self,
        id: i64,
        active: bool,
    ) -> Result<Vec<RecurringTemplateDto>, CoreError> {
        skyla_invoicing::recurring::set_template_active(&self.db(), id, active)?;
        if active {
            self.run_recurring()?;
        }
        self.recurring_templates()
    }

    /// Runs what's due by the books' date. Real books only: the demo's
    /// fixture keeps the state its recordings were made in.
    pub(crate) fn run_recurring(&self) -> Result<usize, CoreError> {
        if self.is_demo() {
            return Ok(0);
        }
        let runs = skyla_invoicing::recurring::run_recurring(
            &self.db(),
            &self.pack,
            &skyla_invoicing::Accounts::cz(),
            &self.domain.entity.as_of,
        )?;
        Ok(runs.len())
    }
}
