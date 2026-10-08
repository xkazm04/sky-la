//! Public reference data (WP-20): ČNB exchange rates and the repo-rate
//! history, imported by hand or fetched when the user opts in (off by
//! default, D-006), and signed rule-pack updates.
//!
//! The only host ever contacted is the ČNB's, and only after the user turns
//! fetching on; each fetch is logged with its URL. A verified pack update
//! applies the next time the books are opened, so figures never change
//! under a report that's open.

use skyla_rules::refdata::{FxDay, RepoChange, fx_on, parse_cnb_daily, parse_repo_history};

use super::Core;
use crate::dto::{PackUpdateDto, RefDataDto, RefSourceDto};
use crate::error::CoreError;

/// The ČNB's daily rates file; `date=DD.MM.YYYY` picks the day.
pub const CNB_DAILY_URL: &str = "https://www.cnb.cz/cs/financni-trhy/devizovy-trh/kurzy-devizoveho-trhu/kurzy-devizoveho-trhu/denni_kurz.txt";

/// Public keys (minisign, base64) trusted to sign rule-pack updates. Empty
/// until the maintainers create the release key, so updates are refused.
pub const TRUSTED_PACK_KEYS: &[&str] = &[];

/// Fetches a URL's text. The app uses HTTPS through `ureq`; tests pass a stub.
pub type Fetcher = Box<dyn Fn(&str) -> Result<String, String> + Send + Sync>;

/// Fetches over HTTPS with a short timeout.
pub fn https_fetcher() -> Fetcher {
    Box::new(|url: &str| {
        if !url.starts_with("https://www.cnb.cz/") {
            return Err(format!("{url} isn't the ČNB's"));
        }
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(15)))
            .build()
            .new_agent();
        agent
            .get(url)
            .call()
            .map_err(|e| e.to_string())?
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())
    })
}

/// What the core holds.
pub(crate) struct RefData {
    pub(crate) fx: Vec<FxDay>,
    pub(crate) repo: Vec<RepoChange>,
    pub(crate) sources: Vec<RefSourceDto>,
    pub(crate) fetch_enabled: bool,
    pub(crate) fetcher: Fetcher,
    pub(crate) pending_pack: Option<String>,
}

impl RefData {
    pub(crate) fn new(fetcher: Fetcher) -> Self {
        Self {
            fx: Vec::new(),
            repo: Vec::new(),
            sources: Vec::new(),
            fetch_enabled: false,
            fetcher,
            pending_pack: None,
        }
    }
}

fn bad(message: impl Into<String>) -> CoreError {
    CoreError::BadRequest(message.into())
}

impl Core {
    pub(crate) fn refdata_state(&self) -> std::sync::MutexGuard<'_, RefData> {
        self.refdata
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Replaces how fetches reach the network (tests, or another transport).
    pub fn replace_fetcher(&self, fetcher: Fetcher) {
        self.refdata_state().fetcher = fetcher;
    }

    /// The ČNB repo history as late interest reads it.
    pub(crate) fn repo_rates(&self) -> Vec<skyla_invoicing::RepoRate> {
        self.refdata_state()
            .repo
            .iter()
            .map(|c| skyla_invoicing::RepoRate {
                effective_from: c.effective_from.clone(),
                rate: c.rate,
            })
            .collect()
    }

    fn add_fx(&self, state: &mut RefData, day: FxDay, origin: String) {
        let summary = format!("{} rates for {}", day.rates.len(), day.date);
        state.fx.retain(|d| d.date != day.date);
        state.sources.push(RefSourceDto {
            kind: "cnb_fx".into(),
            origin,
            summary,
            on: self.domain.entity.as_of.clone(),
        });
        state.fx.push(day);
        state.fx.sort_by(|a, b| a.date.cmp(&b.date));
    }

    /// Imports a reference file the user chose: `cnb_fx` (the ČNB's daily
    /// rates) or `cnb_repo` (the repo-rate history).
    pub fn import_reference_data(
        &self,
        kind: &str,
        file_name: &str,
        text: &str,
    ) -> Result<RefDataDto, CoreError> {
        {
            let mut state = self.refdata_state();
            match kind {
                "cnb_fx" => {
                    let day =
                        parse_cnb_daily(text).map_err(|e| bad(format!("{file_name}: {e}")))?;
                    self.add_fx(&mut state, day, format!("imported {file_name}"));
                }
                "cnb_repo" => {
                    let history =
                        parse_repo_history(text).map_err(|e| bad(format!("{file_name}: {e}")))?;
                    state.sources.push(RefSourceDto {
                        kind: "cnb_repo".into(),
                        origin: format!("imported {file_name}"),
                        summary: format!(
                            "{} changes, {} to {}",
                            history.len(),
                            history
                                .first()
                                .map(|c| c.effective_from.as_str())
                                .unwrap_or_default(),
                            history
                                .last()
                                .map(|c| c.effective_from.as_str())
                                .unwrap_or_default()
                        ),
                        on: self.domain.entity.as_of.clone(),
                    });
                    state.repo = history;
                }
                other => return Err(bad(format!("unknown reference data {other:?}"))),
            }
        }
        self.persist_refdata()?;
        self.reference_data()
    }

    /// Turns fetching from the ČNB on or off. Off is the default.
    pub fn set_reference_fetch(&self, enabled: bool) -> Result<RefDataDto, CoreError> {
        self.refdata_state().fetch_enabled = enabled;
        self.persist_refdata()?;
        self.reference_data()
    }

    /// Fetches the ČNB's rates for `date`, only when the user turned it on.
    pub fn fetch_cnb_rates(&self, date: &str) -> Result<RefDataDto, CoreError> {
        {
            let mut state = self.refdata_state();
            if !state.fetch_enabled {
                return Err(bad(
                    "fetching reference data is off; turn it on in Settings, or import the file",
                ));
            }
            let day = skyla_rules::date::parse(date)
                .ok_or_else(|| bad(format!("{date:?} isn't a date")))?;
            let (y, m, d) = skyla_rules::date::from_days(day);
            let url = format!("{CNB_DAILY_URL}?date={d:02}.{m:02}.{y:04}");
            let text =
                (state.fetcher)(&url).map_err(|e| bad(format!("the ČNB didn't answer: {e}")))?;
            let parsed = parse_cnb_daily(&text)
                .map_err(|e| bad(format!("the ČNB's answer isn't a rates file: {e}")))?;
            self.add_fx(&mut state, parsed, format!("fetched {url}"));
        }
        self.persist_refdata()?;
        self.reference_data()
    }

    /// Checks a signed pack update. A verified one applies when the books
    /// are next opened; a refused one changes nothing.
    pub fn install_pack_update(
        &self,
        pack_toml: &str,
        signature: &str,
    ) -> Result<PackUpdateDto, CoreError> {
        self.install_pack_update_trusting(pack_toml, signature, TRUSTED_PACK_KEYS)
    }

    /// [`Core::install_pack_update`] with the given keys (tests: the
    /// shipped list is empty).
    #[doc(hidden)]
    pub fn install_pack_update_trusting(
        &self,
        pack_toml: &str,
        signature: &str,
        keys: &[&str],
    ) -> Result<PackUpdateDto, CoreError> {
        let pack = skyla_rules::verify_pack_update(pack_toml, signature, keys, &self.pack)
            .map_err(|e| bad(e.to_string()))?;
        let provenance = pack.provenance();
        self.persist_pack(&super::persist::SavedPack {
            toml: pack_toml.to_owned(),
            signature: signature.to_owned(),
        })?;
        self.refdata_state().pending_pack = Some(pack_toml.to_owned());
        Ok(PackUpdateDto {
            in_use: self.pack.provenance(),
            installed: provenance.clone(),
            message: format!("{provenance} is verified and applies when you next open the books."),
        })
    }

    /// What reference data is loaded, where it came from, and the switch.
    pub fn reference_data(&self) -> Result<RefDataDto, CoreError> {
        let state = self.refdata_state();
        let as_of = &self.domain.entity.as_of;
        let euro = fx_on(&state.fx, "EUR", as_of).map(|(day, r)| {
            format!(
                "EUR {} Kč on {} (ČNB #{})",
                r.per_unit().normalize().to_string().replace('.', ","),
                day.date,
                day.number
            )
        });
        let repo = state
            .repo
            .iter()
            .rev()
            .find(|c| c.effective_from.as_str() <= as_of.as_str())
            .map(|c| {
                format!(
                    "{} % since {}",
                    c.rate.normalize().to_string().replace('.', ","),
                    c.effective_from
                )
            });
        Ok(RefDataDto {
            fetch_enabled: state.fetch_enabled,
            fetch_host: "www.cnb.cz".into(),
            fx_days: u32::try_from(state.fx.len()).unwrap_or(u32::MAX),
            euro,
            repo_changes: u32::try_from(state.repo.len()).unwrap_or(u32::MAX),
            repo_now: repo,
            sources: state.sources.clone(),
            trusted_keys: u32::try_from(TRUSTED_PACK_KEYS.len()).unwrap_or(u32::MAX),
            pending_pack: state
                .pending_pack
                .as_ref()
                .map(|_| "an update waits for the next opening".to_owned()),
        })
    }
}
