//! Signed rule-pack updates.
//!
//! A pack update is the pack's TOML and a minisign signature over its exact
//! bytes. It's accepted only when one of the trusted keys signed it, it
//! parses and validates as a pack, it's for the same pack id, and its
//! version is newer than the one in use. Anything else is refused and the
//! current pack stays.

use crate::{Pack, RulesError};

/// Why an update was refused.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// The app ships no key to check updates against.
    #[error(
        "no trusted signing key is configured, so updates can't be checked; nothing was changed"
    )]
    NoTrustedKeys,
    /// The signature file isn't a minisign signature.
    #[error("the signature file isn't a minisign signature: {0}")]
    BadSignature(String),
    /// No trusted key made the signature, or the pack changed after signing.
    #[error(
        "the signature doesn't match the pack: it was changed after signing, or signed by a key sky-la doesn't trust; nothing was changed"
    )]
    Tampered,
    /// The signed pack doesn't validate.
    #[error("the signed pack isn't valid: {0}")]
    Invalid(RulesError),
    /// An update for a different pack.
    #[error("the update is for pack {offered}, not {current}")]
    OtherPack {
        /// In use.
        current: String,
        /// Offered.
        offered: String,
    },
    /// Not newer than the pack in use.
    #[error("the update is version {offered}; {current} is already in use")]
    NotNewer {
        /// In use.
        current: String,
        /// Offered.
        offered: String,
    },
}

fn version_parts(v: &str) -> Vec<u64> {
    v.split(['.', '-'])
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// Checks a pack update against `trusted_keys` (minisign public keys in
/// base64, as in the second line of a `.pub` file) and the pack in use.
pub fn verify_pack_update(
    pack_toml: &str,
    minisig: &str,
    trusted_keys: &[&str],
    current: &Pack,
) -> Result<Pack, UpdateError> {
    if trusted_keys.is_empty() {
        return Err(UpdateError::NoTrustedKeys);
    }
    let signature = minisign_verify::Signature::decode(minisig)
        .map_err(|e| UpdateError::BadSignature(e.to_string()))?;
    let signed = trusted_keys.iter().any(|key| {
        minisign_verify::PublicKey::from_base64(key)
            .is_ok_and(|pk| pk.verify(pack_toml.as_bytes(), &signature, false).is_ok())
    });
    if !signed {
        return Err(UpdateError::Tampered);
    }
    let pack = Pack::from_toml(pack_toml).map_err(UpdateError::Invalid)?;
    if pack.info.id != current.info.id {
        return Err(UpdateError::OtherPack {
            current: current.info.id.clone(),
            offered: pack.info.id.clone(),
        });
    }
    if version_parts(&pack.info.version) <= version_parts(&current.info.version) {
        return Err(UpdateError::NotNewer {
            current: current.info.version.clone(),
            offered: pack.info.version.clone(),
        });
    }
    Ok(pack)
}
