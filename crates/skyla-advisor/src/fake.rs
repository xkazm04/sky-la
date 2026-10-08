//! The `Fake` provider: replays recorded or hand-written stream-json
//! transcripts by task, so advisors, the egress gate and the UI run without
//! a model. Default CI and cloud sessions use only this.

use std::collections::HashMap;

use crate::cli_stream::{StreamEvent, parse_transcript, profile_violations};
use crate::provider::{Availability, LlmProvider, RunOutcome, RunRequest, RunStatus, fold};

/// Scripted answers by task.
#[derive(Debug, Clone)]
pub struct Fake {
    transcripts: HashMap<String, String>,
    availability: Availability,
}

impl Default for Fake {
    fn default() -> Self {
        Self::new()
    }
}

impl Fake {
    /// A ready fake with no scripts.
    pub fn new() -> Self {
        Self {
            transcripts: HashMap::new(),
            availability: Availability::Ready {
                version: "fake".into(),
            },
        }
    }

    /// Answers `task` with `transcript` (stream-json lines).
    #[must_use]
    pub fn with(mut self, task: &str, transcript: &str) -> Self {
        self.transcripts
            .insert(task.to_owned(), transcript.to_owned());
        self
    }

    /// Reports `availability` instead of ready.
    #[must_use]
    pub fn available(mut self, availability: Availability) -> Self {
        self.availability = availability;
        self
    }
}

impl LlmProvider for Fake {
    fn id(&self) -> &str {
        "fake"
    }

    fn availability(&self) -> Availability {
        self.availability.clone()
    }

    fn run(&self, request: &RunRequest, on_event: &mut dyn FnMut(&StreamEvent)) -> RunOutcome {
        match &self.availability {
            Availability::NotInstalled { .. } => {
                return RunOutcome::with_status(RunStatus::NotInstalled);
            }
            Availability::NotSignedIn => return RunOutcome::with_status(RunStatus::NotSignedIn),
            Availability::RateLimited { resets_at } => {
                return RunOutcome::with_status(RunStatus::RateLimited(resets_at.clone()));
            }
            Availability::Ready { .. } => {}
        }
        let Some(text) = self.transcripts.get(&request.task) else {
            return RunOutcome::with_status(RunStatus::Failed(format!(
                "the fake has no transcript for {}",
                request.task
            )));
        };
        let events = match parse_transcript(text) {
            Ok(e) => e,
            Err(e) => return RunOutcome::with_status(RunStatus::Failed(e.to_string())),
        };
        // The same guard as the real driver: a script can't smuggle tools in.
        let mut seen = Vec::new();
        for e in events {
            if let StreamEvent::Init(init) = &e {
                let problems = profile_violations(init, &request.server);
                if !problems.is_empty() {
                    seen.push(e);
                    return RunOutcome {
                        status: RunStatus::ProfileViolation(problems),
                        ..fold(&seen)
                    };
                }
            }
            on_event(&e);
            seen.push(e);
        }
        fold(&seen)
    }
}
