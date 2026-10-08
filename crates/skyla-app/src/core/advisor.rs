//! The advisor provider: which one the core runs on and whether it can run
//! (WP-24). Runs themselves arrive with the advisors (WP-27, WP-28).

use skyla_advisor::{Availability, LlmProvider};

use super::Core;
use crate::dto::AdvisorStatusDto;

/// `1791763200` → `2026-10-08T00:00:00Z`; anything else passes through.
fn rfc3339(unix: &str) -> String {
    let Ok(secs) = unix.parse::<i64>() else {
        return unix.to_owned();
    };
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    format!(
        "{}T{:02}:{:02}:{:02}Z",
        skyla_rules::date::format(days),
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

impl Core {
    /// Swaps the provider (the desktop shell's CLI driver, or a test's fake).
    pub fn replace_provider(&self, provider: Box<dyn LlmProvider>) {
        if let Ok(mut p) = self.provider.lock() {
            *p = provider;
        }
    }

    /// Whether advisors can run now.
    pub fn advisor_status(&self) -> AdvisorStatusDto {
        let (id, availability) = match self.provider.lock() {
            Ok(p) => (p.id().to_owned(), p.availability()),
            Err(_) => (
                "unknown".to_owned(),
                Availability::NotInstalled {
                    looked_in: Vec::new(),
                },
            ),
        };
        let mut dto = AdvisorStatusDto {
            demo: id == "fake",
            provider: id,
            state: String::new(),
            version: None,
            looked_in: Vec::new(),
            resets_at: None,
        };
        match availability {
            Availability::Ready { version } => {
                dto.state = "ready".into();
                dto.version = Some(version);
            }
            Availability::NotInstalled { looked_in } => {
                dto.state = "not_installed".into();
                dto.looked_in = looked_in;
            }
            Availability::NotSignedIn => dto.state = "not_signed_in".into(),
            Availability::RateLimited { resets_at } => {
                dto.state = "rate_limited".into();
                dto.resets_at = resets_at.as_deref().map(rfc3339);
            }
        }
        dto
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unix_seconds_become_utc() {
        assert_eq!(super::rfc3339("1791763200"), "2026-10-12T00:00:00Z");
        assert_eq!(super::rfc3339("1791806400"), "2026-10-12T12:00:00Z");
        assert_eq!(super::rfc3339("soon"), "soon");
    }
}
