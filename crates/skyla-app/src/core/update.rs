//! The opt-in update check (WP-33): the third and last network path
//! CLAUDE.md allows. Off by default. When the user turns it on and asks,
//! the core fetches the project's signed release manifest from GitHub,
//! accepts it only when a trusted release key signed it, and says whether a
//! newer version is out and where to get it. It never downloads or
//! installs anything, and it sends nothing beyond the request itself.

use serde::Deserialize;

use super::Core;
use super::refdata::Fetcher;
use crate::dto::{UpdateDto, UpdateStatusDto};
use crate::error::CoreError;

/// Minisign public keys (base64, the second line of a `.pub` file) trusted
/// to sign releases. Empty until the maintainer creates the release key
/// (`docs/RELEASING.md`); until then no check is made.
pub const TRUSTED_RELEASE_KEYS: &[&str] = &[];

/// Where releases are published; the manifest and its signature sit there.
pub const RELEASES: &str = "https://github.com/xkazm04/sky-la/releases/";
const MANIFEST_URL: &str = "https://github.com/xkazm04/sky-la/releases/latest/download/latest.json";

/// This build's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The signed manifest a release publishes as `latest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// `0.2.0`.
    pub version: String,
    /// `YYYY-MM-DD`.
    pub published: String,
    /// One paragraph.
    pub notes: String,
    /// The release page, under [`RELEASES`].
    pub page: String,
}

fn version_parts(v: &str) -> Vec<u64> {
    v.trim_start_matches('v')
        .split(['.', '-', '+'])
        .take(3)
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// Checks a manifest and its minisign signature against `keys`; the newer
/// release, or `None` when `current` is up to date.
pub fn verify_release(
    manifest: &str,
    minisig: &str,
    keys: &[&str],
    current: &str,
) -> Result<Option<Manifest>, String> {
    if keys.is_empty() {
        return Err(
            "no release signing key is configured yet, so updates can't be verified".into(),
        );
    }
    let signature = minisign_verify::Signature::decode(minisig)
        .map_err(|e| format!("the signature isn't a minisign signature: {e}"))?;
    let signed = keys.iter().any(|key| {
        minisign_verify::PublicKey::from_base64(key)
            .is_ok_and(|pk| pk.verify(manifest.as_bytes(), &signature, false).is_ok())
    });
    if !signed {
        return Err("the release manifest isn't signed by a trusted key; ignoring it".into());
    }
    let m: Manifest = serde_json::from_str(manifest)
        .map_err(|e| format!("the release manifest isn't readable: {e}"))?;
    if !m.page.starts_with(RELEASES) {
        return Err(format!("the release page {} isn't the project's", m.page));
    }
    Ok((version_parts(&m.version) > version_parts(current)).then_some(m))
}

/// Fetches from the project's releases only, over HTTPS, with a timeout.
pub fn release_fetcher() -> Fetcher {
    Box::new(|url: &str| {
        if !url.starts_with(RELEASES) {
            return Err(format!("{url} isn't the project's releases"));
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
pub(crate) struct UpdateState {
    pub(crate) enabled: bool,
    pub(crate) available: Option<Manifest>,
    pub(crate) checked: bool,
    pub(crate) fetcher: Fetcher,
}

impl UpdateState {
    pub(crate) fn new(fetcher: Fetcher) -> Self {
        Self {
            enabled: false,
            available: None,
            checked: false,
            fetcher,
        }
    }
}

impl Core {
    fn update_state(&self) -> std::sync::MutexGuard<'_, UpdateState> {
        self.updates
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether checks are on, and what the last one found.
    pub fn update_status(&self) -> UpdateStatusDto {
        let s = self.update_state();
        UpdateStatusDto {
            enabled: s.enabled,
            current: VERSION.into(),
            source: "github.com".into(),
            trusted_keys: u32::try_from(TRUSTED_RELEASE_KEYS.len()).unwrap_or(u32::MAX),
            checked: s.checked,
            available: s.available.as_ref().map(|m| UpdateDto {
                version: m.version.clone(),
                published: m.published.clone(),
                notes: m.notes.clone(),
                page: m.page.clone(),
            }),
        }
    }

    /// Turns the update check on or off.
    pub fn set_update_check(&self, enabled: bool) -> UpdateStatusDto {
        {
            let mut s = self.update_state();
            s.enabled = enabled;
            if !enabled {
                s.available = None;
                s.checked = false;
            }
        }
        self.update_status()
    }

    /// Checks for a newer version, only when the user turned it on.
    pub fn check_for_update(&self) -> Result<UpdateStatusDto, CoreError> {
        self.check_for_update_with(TRUSTED_RELEASE_KEYS)
    }

    pub(crate) fn check_for_update_with(
        &self,
        keys: &[&str],
    ) -> Result<UpdateStatusDto, CoreError> {
        {
            let mut s = self.update_state();
            if !s.enabled {
                return Err(CoreError::BadRequest(
                    "the update check is off; turn it on in Settings first".into(),
                ));
            }
            if keys.is_empty() {
                return Err(CoreError::BadRequest(
                    "no release signing key is configured yet, so sky-la doesn't check for updates; nothing was fetched".into(),
                ));
            }
            let manifest = (s.fetcher)(MANIFEST_URL).map_err(|e| {
                CoreError::BadRequest(format!("the release page didn't answer: {e}"))
            })?;
            let signature = (s.fetcher)(&format!("{MANIFEST_URL}.minisig")).map_err(|e| {
                CoreError::BadRequest(format!("the release page didn't answer: {e}"))
            })?;
            let found = verify_release(&manifest, &signature, keys, VERSION)
                .map_err(CoreError::BadRequest)?;
            s.available = found;
            s.checked = true;
        }
        Ok(self.update_status())
    }

    /// Replaces the fetcher (tests).
    #[doc(hidden)]
    pub fn replace_update_fetcher(&self, fetcher: Fetcher) {
        self.update_state().fetcher = fetcher;
    }

    /// Checks with the given keys (tests: the shipped list is empty).
    #[doc(hidden)]
    pub fn check_for_update_trusting(&self, keys: &[&str]) -> Result<UpdateStatusDto, CoreError> {
        self.check_for_update_with(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::version_parts;

    #[test]
    fn versions_compare_numerically() {
        assert!(version_parts("0.10.0") > version_parts("0.9.3"));
        assert!(version_parts("v1.0.0") > version_parts("0.99.0"));
        assert_eq!(version_parts("0.2.0-rc.1"), [0, 2, 0]);
    }
}
