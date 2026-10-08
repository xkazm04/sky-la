//! WP-33 acceptance (update channel): off by default; with no trusted
//! release key nothing is fetched; a manifest is believed only when a
//! trusted key signed it and it points at the project's releases; nothing
//! is ever downloaded beyond the manifest and its signature.

#![allow(clippy::unwrap_used)]

use std::io::Cursor;
use std::sync::{Arc, Mutex};

use skyla_app::Core;
use skyla_app::update::{VERSION, verify_release};

fn keys() -> (minisign::PublicKey, minisign::SecretKey) {
    let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    (kp.pk, kp.sk)
}

fn sign(text: &str, pk: &minisign::PublicKey, sk: &minisign::SecretKey) -> String {
    minisign::sign(
        Some(pk),
        sk,
        Cursor::new(text.as_bytes()),
        Some("sky-la release"),
        None,
    )
    .unwrap()
    .to_string()
}

fn manifest(version: &str, page: &str) -> String {
    format!(
        r#"{{"version":"{version}","published":"2026-11-02","notes":"Credit notes import.","page":"{page}"}}"#
    )
}

const PAGE: &str = "https://github.com/xkazm04/sky-la/releases/tag/v9.0.0";

#[test]
fn only_a_signed_manifest_for_the_projects_releases_is_believed() {
    let (pk, sk) = keys();
    let key = pk.to_base64();
    let m = manifest("9.0.0", PAGE);
    let found = verify_release(&m, &sign(&m, &pk, &sk), &[&key], VERSION).unwrap();
    assert_eq!(found.unwrap().version, "9.0.0");

    let same = manifest(VERSION, PAGE);
    assert_eq!(
        verify_release(&same, &sign(&same, &pk, &sk), &[&key], VERSION).unwrap(),
        None
    );

    let tampered = m.replace("9.0.0", "9.0.1");
    assert!(verify_release(&tampered, &sign(&m, &pk, &sk), &[&key], VERSION).is_err());

    let (other, _) = keys();
    assert!(verify_release(&m, &sign(&m, &pk, &sk), &[&other.to_base64()], VERSION).is_err());

    let elsewhere = manifest("9.0.0", "https://evil.example/sky-la.dmg");
    assert!(verify_release(&elsewhere, &sign(&elsewhere, &pk, &sk), &[&key], VERSION).is_err());

    assert!(
        verify_release(&m, &sign(&m, &pk, &sk), &[], VERSION).is_err(),
        "no keys, no trust"
    );
}

#[test]
fn the_check_is_off_by_default_and_fetches_nothing_without_a_key() {
    let core = Core::demo().unwrap();
    let asked = Arc::new(Mutex::new(Vec::<String>::new()));
    let log = asked.clone();
    core.replace_update_fetcher(Box::new(move |url| {
        log.lock().unwrap().push(url.to_owned());
        Err("offline".into())
    }));
    let status = core.update_status();
    assert!(!status.enabled);
    assert_eq!(status.trusted_keys, 0);
    assert!(core.check_for_update().is_err(), "off");
    core.set_update_check(true);
    let refused = core.check_for_update().unwrap_err().to_string();
    assert!(refused.contains("no release signing key"), "{refused}");
    assert!(asked.lock().unwrap().is_empty(), "nothing was fetched");
}

#[test]
fn a_trusted_newer_release_is_reported_with_its_page() {
    let (pk, sk) = keys();
    let key = pk.to_base64();
    let m = manifest("9.0.0", PAGE);
    let sig = sign(&m, &pk, &sk);
    let core = Core::demo().unwrap();
    let asked = Arc::new(Mutex::new(Vec::<String>::new()));
    let log = asked.clone();
    core.replace_update_fetcher(Box::new(move |url| {
        log.lock().unwrap().push(url.to_owned());
        Ok(if url.ends_with(".minisig") {
            sig.clone()
        } else {
            m.clone()
        })
    }));
    core.set_update_check(true);
    let status = core.check_for_update_trusting(&[&key]).unwrap();
    assert!(status.checked);
    assert_eq!(status.available.unwrap().page, PAGE);
    let urls = asked.lock().unwrap().clone();
    assert_eq!(
        urls.len(),
        2,
        "the manifest and its signature, nothing else"
    );
    assert!(
        urls.iter()
            .all(|u| u.starts_with("https://github.com/xkazm04/sky-la/releases/"))
    );
    // Turning it off forgets what it found.
    assert!(core.set_update_check(false).available.is_none());
}
